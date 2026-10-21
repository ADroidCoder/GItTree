//! 代码托管平台的设置（每个主机是哪种平台、访问令牌）和 PR 列表的联网请求。
//! 设置存在应用数据目录里的 forges.json，只有本机用户能读。

use gittree_core::forge::{self, Kind, Project, PullRequest};
use gittree_core::tr;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

#[derive(Default, Serialize, Deserialize)]
struct Settings {
    #[serde(default)]
    hosts: HashMap<String, HostSettings>,
}

#[derive(Default, Clone, Serialize, Deserialize)]
struct HostSettings {
    kind: Option<Kind>,
    token: Option<String>,
}

fn settings_file(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("forges.json"))
}

fn load(app: &AppHandle) -> Settings {
    settings_file(app)
        .ok()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|d| serde_json::from_slice(&d).ok())
        .unwrap_or_default()
}

fn save(app: &AppHandle, s: &Settings) -> Result<(), String> {
    let path = settings_file(app)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(s).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    // 里面有访问令牌：只允许自己读写
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct ForgeInfo {
    project: Project,
    /// 认出来的或用户选的平台；None 表示需要用户选
    kind: Option<Kind>,
    has_token: bool,
    /// 创建 PR 的网页地址（需要 kind）
    pulls_web: Option<String>,
    token_page: Option<String>,
}

fn info(app: &AppHandle, url: &str) -> Result<ForgeInfo, String> {
    let project = forge::project(url).ok_or_else(|| tr!("认不出 {url} 是哪个仓库", "Can't recognize the repository {url}"))?;
    let host = load(app).hosts.get(&project.host).cloned().unwrap_or_default();
    let kind = host.kind.or_else(|| forge::guess(&project.host));
    Ok(ForgeInfo {
        pulls_web: kind.map(|k| forge::pulls_web_url(k, &project)),
        token_page: kind.map(|k| forge::token_page(k, &project)),
        has_token: host.token.as_deref().is_some_and(|t| !t.is_empty()),
        kind,
        project,
    })
}

#[tauri::command]
pub fn forge_info(app: AppHandle, url: String) -> Result<ForgeInfo, String> {
    info(&app, &url)
}

/// 创建 PR 的网页地址
#[tauri::command]
pub fn forge_new_pull_url(app: AppHandle, url: String, source: String, target: String) -> Result<String, String> {
    let i = info(&app, &url)?;
    let kind = i.kind.ok_or_else(|| tr!("请先选择代码托管平台", "Choose the hosting service first"))?;
    Ok(forge::new_pull_url(kind, &i.project, &source, &target))
}

/// 设置主机的平台类型和访问令牌。token 为 None 时不改，为空字符串时删除
#[tauri::command]
pub fn forge_configure(app: AppHandle, host: String, kind: Option<Kind>, token: Option<String>) -> Result<(), String> {
    let mut s = load(&app);
    let h = s.hosts.entry(host).or_default();
    if kind.is_some() {
        h.kind = kind;
    }
    if let Some(t) = token {
        h.token = Some(t.trim().to_string()).filter(|t| !t.is_empty());
    }
    save(&app, &s)
}

#[derive(Serialize)]
pub struct Pulls {
    kind: Kind,
    pulls: Vec<PullRequest>,
}

/// 打开中的 PR / MR。需要令牌时错误信息以 need_token: 开头，界面据此提示设置令牌
#[tauri::command]
pub async fn pull_requests(app: AppHandle, url: String) -> Result<Pulls, String> {
    let i = info(&app, &url)?;
    let kind = i.kind.ok_or_else(|| tr!("请先选择代码托管平台", "Choose the hosting service first"))?;
    let token = load(&app).hosts.get(&i.project.host).and_then(|h| h.token.clone());
    let mut api = forge::pulls_api_url(kind, &i.project);

    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
    let client = reqwest::Client::builder()
        .user_agent(concat!("GitTree/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;
    if let (Kind::Gitee, Some(t)) = (kind, &token) {
        api.push_str(&format!("&access_token={t}"));
    }
    let mut req = client.get(&api);
    if let Some(t) = &token {
        req = match kind {
            Kind::Github => req.bearer_auth(t).header("Accept", "application/vnd.github+json"),
            Kind::Gitlab => req.header("PRIVATE-TOKEN", t),
            Kind::Gitea => req.header("Authorization", format!("token {t}")),
            Kind::Gitee => req,
        };
    }
    // 报错信息里可能带着地址，Gitee 的令牌在地址里，不能显示出来
    let scrub = |s: String| match &token {
        Some(t) if !t.is_empty() => s.replace(t.as_str(), "***"),
        _ => s,
    };
    let res = req.send().await.map_err(|e| scrub(tr!("连接 {} 失败：{e}", "Couldn't reach {}: {e}", i.project.host)))?;
    let status = res.status();
    let body = res.bytes().await.map_err(|e| scrub(e.to_string()))?;
    // GitHub 没有令牌时每小时只能请求 60 次，超了返回 403 / 429
    let text = String::from_utf8_lossy(&body);
    if matches!(status.as_u16(), 403 | 429) && text.to_ascii_lowercase().contains("rate limit") {
        return Err(format!(
            "need_token:{}",
            tr!(
                "请求太频繁，被 {} 暂时限制了（不登录时每小时只能请求几十次）。设置访问令牌后限额会高很多，也可以过一会儿再试",
                "Rate limited by {} (only a few dozen requests per hour without signing in). Set an access token for a much higher limit, or try again later",
                i.project.host
            )
        ));
    }
    // 私有仓库没有令牌时，GitHub / Gitee 返回 404，GitLab 返回 404 或 401
    if matches!(status.as_u16(), 401 | 403 | 404) {
        let hint = if token.is_some() {
            tr!("访问被拒绝（{status}），令牌可能过期了或者没有读取仓库的权限", "Access denied ({status}). The token may have expired or lack permission to read the repository")
        } else {
            tr!("这个仓库需要访问令牌才能查看（{status}）", "This repository needs an access token ({status})")
        };
        return Err(format!("need_token:{hint}"));
    }
    if !status.is_success() {
        return Err(tr!("{} 返回错误 {status}：{}", "{} returned {status}: {}", i.project.host, text.chars().take(300).collect::<String>()));
    }
    let json: serde_json::Value = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
    Ok(Pulls { kind, pulls: forge::parse_pulls(kind, &json) })
}
