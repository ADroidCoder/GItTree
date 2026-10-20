//! 需要用户名、密码、token 或 SSH 密钥密码时，在界面上弹窗询问。
//!
//! git 和 ssh 需要输入时会运行 GIT_ASKPASS / SSH_ASKPASS 指向的程序，把提示文字作为参数，
//! 程序打印出来的内容就是答案。这里让 GitTree 自己充当这个程序：
//! 以“询问模式”启动时连到主程序监听的本机端口，主程序弹窗，用户填完后把答案传回来。
//! 登录成功后 git 会用系统的凭据助手（钥匙串 / Windows 凭据管理器）记住，下次不用再填。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::{BuildHasher, Hasher};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock, mpsc};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

const PORT_ENV: &str = "GITTREE_ASKPASS_PORT";
const SECRET_ENV: &str = "GITTREE_ASKPASS_SECRET";
/// 用户一直不回答时，最多等这么久
const WAIT: Duration = Duration::from_secs(600);

#[derive(Serialize, Deserialize)]
struct Request {
    secret: String,
    prompt: String,
}

#[derive(Serialize, Deserialize)]
struct Reply {
    answer: Option<String>,
}

/// 被 git / ssh 当作询问程序启动时，问完就退出；正常启动时返回 None
pub fn helper_main() -> Option<i32> {
    let port = std::env::var(PORT_ENV).ok()?;
    let secret = std::env::var(SECRET_ENV).unwrap_or_default();
    let prompt = std::env::args().nth(1).unwrap_or_default();
    let ask = || -> std::io::Result<Option<String>> {
        let mut stream = TcpStream::connect(("127.0.0.1", port.parse::<u16>().unwrap_or(0)))?;
        let req = serde_json::to_string(&Request { secret, prompt })?;
        stream.write_all(req.as_bytes())?;
        stream.write_all(b"\n")?;
        let mut line = String::new();
        BufReader::new(stream).read_line(&mut line)?;
        Ok(serde_json::from_str::<Reply>(&line)?.answer)
    };
    Some(match ask() {
        Ok(Some(answer)) => {
            println!("{answer}");
            0
        }
        // 用户取消或主程序没响应：退出码非 0，git 会当作没有拿到凭据
        _ => 1,
    })
}

#[derive(Clone, Serialize)]
struct AskEvent {
    id: u64,
    prompt: String,
}

#[derive(Default)]
struct State {
    next: AtomicU64,
    pending: Mutex<HashMap<u64, mpsc::Sender<Option<String>>>>,
}

static STATE: OnceLock<State> = OnceLock::new();

fn random_secret() -> String {
    // RandomState 每次都用系统随机数做种子，够用来防止本机其他程序冒充
    let s = std::collections::hash_map::RandomState::new();
    let (mut a, mut b) = (s.build_hasher(), s.build_hasher());
    a.write_u64(1);
    b.write_u64(2);
    format!("{:016x}{:016x}", a.finish(), b.finish())
}

/// 启动本机监听，返回要加到每个 git 命令里的环境变量
pub fn start(app: AppHandle) -> Vec<(String, String)> {
    let Ok(listener) = TcpListener::bind(("127.0.0.1", 0)) else { return Vec::new() };
    let Ok(port) = listener.local_addr().map(|a| a.port()) else { return Vec::new() };
    let Ok(exe) = std::env::current_exe() else { return Vec::new() };
    let secret = random_secret();
    let state = STATE.get_or_init(State::default);
    let expected = secret.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let app = app.clone();
            let expected = expected.clone();
            std::thread::spawn(move || handle(stream, &app, state, &expected));
        }
    });
    let exe = exe.to_string_lossy().into_owned();
    vec![
        ("GIT_ASKPASS".into(), exe.clone()),
        ("SSH_ASKPASS".into(), exe),
        // OpenSSH 8.4 以后：没有终端时也使用 SSH_ASKPASS
        ("SSH_ASKPASS_REQUIRE".into(), "force".into()),
        (PORT_ENV.into(), port.to_string()),
        (SECRET_ENV.into(), secret),
    ]
}

fn handle(stream: TcpStream, app: &AppHandle, state: &State, expected: &str) {
    let mut reader = BufReader::new(&stream);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return;
    }
    let Ok(req) = serde_json::from_str::<Request>(&line) else { return };
    if req.secret != expected {
        return;
    }
    let id = state.next.fetch_add(1, Ordering::SeqCst) + 1;
    let (tx, rx) = mpsc::channel();
    if let Ok(mut p) = state.pending.lock() {
        p.insert(id, tx);
    }
    let _ = app.emit("askpass", AskEvent { id, prompt: req.prompt.trim().to_string() });
    let answer = rx.recv_timeout(WAIT).ok().flatten();
    if let Ok(mut p) = state.pending.lock() {
        p.remove(&id);
    }
    let reply = serde_json::to_string(&Reply { answer }).unwrap_or_default();
    let mut w = &stream;
    let _ = w.write_all(reply.as_bytes());
    let _ = w.write_all(b"\n");
}

/// 界面上用户填完（answer）或取消（None）
#[tauri::command]
pub fn askpass_reply(id: u64, answer: Option<String>) {
    if let Some(state) = STATE.get()
        && let Ok(p) = state.pending.lock()
        && let Some(tx) = p.get(&id)
    {
        let _ = tx.send(answer);
    }
}
