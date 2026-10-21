"""生成自动更新用的 latest.json。用法: python3 latest_json.py <安装包目录> <标签> <仓库>，更新说明从标准输入读"""
import datetime
import json
import pathlib
import sys

dist, tag, repo = pathlib.Path(sys.argv[1]), sys.argv[2], sys.argv[3]
base = f"https://github.com/{repo}/releases/download/{tag}/"

# 平台 → 更新包文件名的匹配规则
patterns = {
    "darwin-aarch64": "*_aarch64.app.tar.gz",
    "darwin-x86_64": "*_x64.app.tar.gz",
    "windows-x86_64": "*-setup.exe",
}
platforms = {}
for platform, pattern in patterns.items():
    for f in dist.glob(pattern):
        sig = f.with_name(f.name + ".sig")
        if sig.exists():
            platforms[platform] = {"signature": sig.read_text().strip(), "url": base + f.name}

if not platforms:
    sys.exit("没有找到签名过的更新包，跳过 latest.json（仓库的 Secrets 里没有配置 TAURI_SIGNING_PRIVATE_KEY？）")

print(
    json.dumps(
        {
            "version": tag.lstrip("v"),
            "notes": sys.stdin.read().strip(),
            "pub_date": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
            "platforms": platforms,
        },
        ensure_ascii=False,
        indent=2,
    )
)
