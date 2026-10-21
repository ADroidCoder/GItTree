//! 代码托管平台（GitHub、GitLab、Gitee、Gitea）：从远程地址认出平台和项目，
//! 生成创建 PR 的网页地址、PR 列表的接口地址，解析接口返回的 PR。
//! 联网请求在界面程序里做，这里只有纯粹的转换，方便测试。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Github,
    Gitlab,
    Gitee,
    Gitea,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Project {
    /// 网页的主机名（可能带端口），例如 github.com、gitlab.example.com:8443
    pub host: String,
    /// owner/repo，GitLab 可以有多级：group/sub/repo
    pub path: String,
    /// 网页地址，例如 https://github.com/owner/repo
    pub web: String,
}

/// 从远程地址里取出主机和项目路径：
/// git@host:a/b.git、ssh://git@host:22/a/b.git、https://user@host/a/b.git
pub fn project(url: &str) -> Option<Project> {
    let url = url.trim();
    let (scheme_http, rest) = if let Some(r) = url.strip_prefix("https://") {
        (Some("https"), r)
    } else if let Some(r) = url.strip_prefix("http://") {
        (Some("http"), r)
    } else if let Some(r) = url.strip_prefix("ssh://").or_else(|| url.strip_prefix("git://")) {
        (None, r)
    } else if !url.contains("://") && url.contains(':') {
        // scp 写法 git@host:owner/repo.git
        let (host, path) = url.split_once(':')?;
        let host = host.rsplit('@').next()?;
        return make(host, path, "https");
    } else {
        return None;
    };
    let (authority, path) = rest.split_once('/')?;
    let host = authority.rsplit('@').next()?;
    match scheme_http {
        Some(s) => make(host, path, s),
        // ssh 的端口不是网页的端口
        None => make(host.split(':').next()?, path, "https"),
    }
}

fn make(host: &str, path: &str, scheme: &str) -> Option<Project> {
    let path = path.trim_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path).trim_end_matches('/');
    if host.is_empty() || !path.contains('/') {
        return None;
    }
    Some(Project { host: host.to_string(), path: path.to_string(), web: format!("{scheme}://{host}/{path}") })
}

/// 从主机名猜平台；猜不出来（公司自己部署的）时返回 None，由用户选择
pub fn guess(host: &str) -> Option<Kind> {
    let h = host.to_ascii_lowercase();
    let h = h.split(':').next().unwrap_or(&h);
    if h == "github.com" || h.ends_with(".ghe.com") || h.starts_with("github.") {
        Some(Kind::Github)
    } else if h == "gitee.com" {
        Some(Kind::Gitee)
    } else if h.contains("gitlab") || h == "jihulab.com" {
        Some(Kind::Gitlab)
    } else if h.contains("gitea") || h == "codeberg.org" || h.contains("forgejo") {
        Some(Kind::Gitea)
    } else {
        None
    }
}

/// 创建 PR / MR 的网页地址
pub fn new_pull_url(kind: Kind, p: &Project, source: &str, target: &str) -> String {
    let (s, t) = (enc(source), enc(target));
    match kind {
        Kind::Github => format!("{}/compare/{t}...{s}?expand=1", p.web),
        Kind::Gitlab => format!("{}/-/merge_requests/new?merge_request%5Bsource_branch%5D={s}&merge_request%5Btarget_branch%5D={t}", p.web),
        Kind::Gitee => {
            let owner = p.path.split('/').next().unwrap_or("");
            format!("{}/pull/new/{owner}:{s}...{owner}:{t}", p.web)
        }
        Kind::Gitea => format!("{}/compare/{t}...{s}", p.web),
    }
}

/// PR 列表的网页地址
pub fn pulls_web_url(kind: Kind, p: &Project) -> String {
    match kind {
        Kind::Gitlab => format!("{}/-/merge_requests", p.web),
        Kind::Github | Kind::Gitee | Kind::Gitea => format!("{}/pulls", p.web),
    }
}

/// 打开中的 PR 列表接口
pub fn pulls_api_url(kind: Kind, p: &Project) -> String {
    let scheme = if p.web.starts_with("http://") { "http" } else { "https" };
    let host = &p.host;
    match kind {
        Kind::Github if host == "github.com" => format!("https://api.github.com/repos/{}/pulls?state=open&per_page=100", p.path),
        // GitHub Enterprise
        Kind::Github => format!("{scheme}://{host}/api/v3/repos/{}/pulls?state=open&per_page=100", p.path),
        Kind::Gitlab => format!("{scheme}://{host}/api/v4/projects/{}/merge_requests?state=opened&per_page=100", enc(&p.path)),
        Kind::Gitee => format!("https://gitee.com/api/v5/repos/{}/pulls?state=open&per_page=100", p.path),
        Kind::Gitea => format!("{scheme}://{host}/api/v1/repos/{}/pulls?state=open&limit=50", p.path),
    }
}

/// 创建访问令牌的网页
pub fn token_page(kind: Kind, p: &Project) -> String {
    let scheme = if p.web.starts_with("http://") { "http" } else { "https" };
    match kind {
        Kind::Github if p.host == "github.com" => "https://github.com/settings/tokens".into(),
        Kind::Github => format!("{scheme}://{}/settings/tokens", p.host),
        Kind::Gitlab => format!("{scheme}://{}/-/user_settings/personal_access_tokens", p.host),
        Kind::Gitee => "https://gitee.com/profile/personal_access_tokens".into(),
        Kind::Gitea => format!("{scheme}://{}/user/settings/applications", p.host),
    }
}

/// 把 PR 的代码拉到本地用的远程引用
pub fn pull_ref(kind: Kind, number: u64) -> String {
    match kind {
        Kind::Gitlab => format!("refs/merge-requests/{number}/head"),
        Kind::Github | Kind::Gitee | Kind::Gitea => format!("refs/pull/{number}/head"),
    }
}

#[derive(Debug, Serialize, PartialEq)]
pub struct PullRequest {
    pub number: u64,
    pub title: String,
    pub author: String,
    pub source: String,
    pub target: String,
    /// 来自 fork 仓库
    pub fork: bool,
    /// fork 仓库的 owner/repo（GitLab 的列表接口里没有，为空）
    pub source_repo: Option<String>,
    pub draft: bool,
    pub updated: String,
    pub url: String,
}

/// 解析 PR 列表接口返回的 JSON
pub fn parse_pulls(kind: Kind, json: &serde_json::Value) -> Vec<PullRequest> {
    let s = |v: &serde_json::Value, path: &[&str]| {
        let mut cur = v;
        for k in path {
            cur = &cur[*k];
        }
        cur.as_str().unwrap_or("").to_string()
    };
    let Some(arr) = json.as_array() else { return Vec::new() };
    arr.iter()
        .map(|v| match kind {
            Kind::Gitlab => PullRequest {
                number: v["iid"].as_u64().unwrap_or(0),
                title: s(v, &["title"]),
                author: s(v, &["author", "username"]),
                source: s(v, &["source_branch"]),
                target: s(v, &["target_branch"]),
                fork: v["source_project_id"] != v["target_project_id"],
                source_repo: None,
                draft: v["draft"].as_bool().or(v["work_in_progress"].as_bool()).unwrap_or(false),
                updated: s(v, &["updated_at"]),
                url: s(v, &["web_url"]),
            },
            _ => {
                let base_repo = s(v, &["base", "repo", "full_name"]);
                let head_repo = s(v, &["head", "repo", "full_name"]);
                PullRequest {
                    number: v["number"].as_u64().unwrap_or(0),
                    title: s(v, &["title"]),
                    author: s(v, &["user", "login"]),
                    source: s(v, &["head", "ref"]),
                    target: s(v, &["base", "ref"]),
                    // fork 被删掉后 head.repo 是 null，也当作 fork
                    fork: head_repo != base_repo,
                    source_repo: (!head_repo.is_empty() && head_repo != base_repo).then_some(head_repo),
                    draft: v["draft"].as_bool().unwrap_or(false),
                    updated: s(v, &["updated_at"]),
                    url: s(v, &["html_url"]),
                }
            }
        })
        .collect()
}

/// 网址里的一段，分支名里的 / 等字符要转义
fn enc(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_remote_urls() {
        let p = project("git@github.com:ADroidCoder/GItTree.git").unwrap();
        assert_eq!((p.host.as_str(), p.path.as_str(), p.web.as_str()), ("github.com", "ADroidCoder/GItTree", "https://github.com/ADroidCoder/GItTree"));
        let p = project("ssh://git@gitlab.corp.com:2222/group/sub/app.git").unwrap();
        assert_eq!((p.host.as_str(), p.path.as_str()), ("gitlab.corp.com", "group/sub/app"));
        let p = project("https://user:token@gitee.com/a/b.git").unwrap();
        assert_eq!(p.web, "https://gitee.com/a/b");
        let p = project("http://10.0.0.5:8080/team/app").unwrap();
        assert_eq!(p.web, "http://10.0.0.5:8080/team/app");
        assert!(project("/local/path/repo").is_none());
    }

    #[test]
    fn urls_for_each_forge() {
        let p = project("git@gitlab.corp.com:group/sub/app.git").unwrap();
        assert_eq!(guess(&p.host), Some(Kind::Gitlab));
        assert_eq!(
            pulls_api_url(Kind::Gitlab, &p),
            "https://gitlab.corp.com/api/v4/projects/group%2Fsub%2Fapp/merge_requests?state=opened&per_page=100"
        );
        assert!(new_pull_url(Kind::Gitlab, &p, "feature/x", "main").contains("source_branch%5D=feature%2Fx&"));
        let p = project("git@github.com:a/b.git").unwrap();
        assert_eq!(new_pull_url(Kind::Github, &p, "dev", "main"), "https://github.com/a/b/compare/main...dev?expand=1");
        assert_eq!(pulls_api_url(Kind::Github, &p), "https://api.github.com/repos/a/b/pulls?state=open&per_page=100");
        let p = project("git@gitee.com:a/b.git").unwrap();
        assert_eq!(new_pull_url(Kind::Gitee, &p, "dev", "master"), "https://gitee.com/a/b/pull/new/a:dev...a:master");
    }

    #[test]
    fn parses_pull_lists() {
        let gh: serde_json::Value = serde_json::from_str(
            r#"[{"number":7,"title":"修复","user":{"login":"bob"},"head":{"ref":"fix","repo":{"full_name":"bob/b"}},"base":{"ref":"main","repo":{"full_name":"a/b"}},"draft":true,"updated_at":"2026-01-01T00:00:00Z","html_url":"https://github.com/a/b/pull/7"}]"#,
        )
        .unwrap();
        let prs = parse_pulls(Kind::Github, &gh);
        assert_eq!(prs[0].number, 7);
        assert_eq!(prs[0].source_repo.as_deref(), Some("bob/b"));
        assert!(prs[0].fork);
        assert!(prs[0].draft);
        let gl: serde_json::Value = serde_json::from_str(
            r#"[{"iid":3,"title":"新功能","author":{"username":"amy"},"source_branch":"feat","target_branch":"main","source_project_id":1,"target_project_id":1,"draft":false,"updated_at":"x","web_url":"u"}]"#,
        )
        .unwrap();
        let prs = parse_pulls(Kind::Gitlab, &gl);
        assert_eq!((prs[0].number, prs[0].source.as_str(), prs[0].fork), (3, "feat", false));
        assert!(prs[0].source_repo.is_none());
    }
}
