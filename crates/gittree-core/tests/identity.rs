//! 提交身份。会改全局配置，所以单独一个测试程序，并把全局配置指到临时文件。

use gittree_core::{identity, ops};

#[test]
fn profiles_switch_identity_by_directory() {
    let root = std::env::temp_dir().join(format!("gittree-identity-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let global = root.join("global.gitconfig");
    std::fs::write(&global, "[user]\n\tname = 个人\n\temail = me@home.com\n").unwrap();
    // SAFETY: 这个测试程序只有这一个测试，设置时还没有别的线程
    unsafe {
        std::env::set_var("GIT_CONFIG_GLOBAL", &global);
        std::env::set_var("GIT_CONFIG_NOSYSTEM", "1");
    }
    let store = root.join("profiles dir");
    let work = root.join("company work");
    let repo = work.join("app");
    std::fs::create_dir_all(&repo).unwrap();
    ops::git(&repo, &["init", "-q"]).unwrap();
    let other = root.join("other");
    std::fs::create_dir_all(&other).unwrap();
    ops::git(&other, &["init", "-q"]).unwrap();

    let id = identity::current(&repo);
    assert_eq!(id.email.as_deref(), Some("me@home.com"));
    assert_eq!(id.scope.as_deref(), Some("global"));

    // 新建一套公司身份，并让 company work 目录下的仓库自动使用
    let pid = identity::save_profile(&store, None, "公司", "张三", "zhangsan@corp.com", None).unwrap();
    identity::set_dirs(&store, &pid, &[work.to_string_lossy().into_owned()]).unwrap();
    let list = identity::profiles(&store).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!((list[0].label.as_str(), list[0].email.as_str()), ("公司", "zhangsan@corp.com"));
    assert_eq!(list[0].dirs, [work.to_string_lossy().replace('\\', "/")]);

    let id = identity::current(&repo);
    assert_eq!(id.name.as_deref(), Some("张三"));
    assert_eq!(id.email.as_deref(), Some("zhangsan@corp.com"));
    assert!(id.origin.unwrap().ends_with(".gitconfig"));
    // 别的目录不受影响
    assert_eq!(identity::current(&other).email.as_deref(), Some("me@home.com"));

    // 本仓库单独设置优先
    identity::set_local(&repo, "临时", "tmp@x.com").unwrap();
    assert_eq!(identity::current(&repo).scope.as_deref(), Some("local"));
    identity::clear_local(&repo).unwrap();
    identity::clear_local(&repo).unwrap(); // 没有设置时也不报错
    assert_eq!(identity::current(&repo).email.as_deref(), Some("zhangsan@corp.com"));

    // 修改邮箱立即生效；删除身份后目录规则也去掉
    identity::save_profile(&store, Some(&pid), "公司", "张三", "zs@corp.com", Some("ABC123")).unwrap();
    assert_eq!(identity::current(&repo).email.as_deref(), Some("zs@corp.com"));
    assert_eq!(identity::profiles(&store).unwrap()[0].signing_key.as_deref(), Some("ABC123"));
    identity::delete_profile(&store, &pid).unwrap();
    assert!(identity::profiles(&store).unwrap().is_empty());
    assert_eq!(identity::current(&repo).email.as_deref(), Some("me@home.com"));
    assert!(!std::fs::read_to_string(&global).unwrap().contains("includeIf"));
}
