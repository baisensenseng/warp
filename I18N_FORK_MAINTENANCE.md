# 中文多语言分支维护流程

## 维护目标

本仓库基于官方 Warp 仓库维护中文多语言功能。

目标是持续同步官方 `master` 的最新代码，同时保留本仓库的中文多语言改动。

## 远程仓库职责

```text
upstream = 官方仓库，只用于拉取官方更新
origin   = 自己的仓库，用于保存和推送中文多语言分支
```

当前远程配置应保持为：

```bash
git remote -v
```

期望结果：

```text
origin    https://github.com/baisensenseng/warp.git
upstream  https://github.com/warpdotdev/warp.git
```

`upstream` 不用于推送，避免误推官方仓库。

## 分支职责

```text
master              跟随官方 upstream/master，不放自定义功能
feature/i18n-zh-cn  中文多语言功能长期维护分支
```

中文功能只维护在 `feature/i18n-zh-cn`。

## 日常同步官方代码

每次准备继续开发、修复问题或重新打包前，先同步官方代码。

```bash
git fetch upstream
git checkout master
git merge --ff-only upstream/master
git checkout feature/i18n-zh-cn
git rebase master
git push --force-with-lease origin feature/i18n-zh-cn
```

说明：

- `fetch upstream` 拉取官方最新代码
- `master` 只做快进更新，保持和官方一致
- `feature/i18n-zh-cn` 通过 `rebase master` 把中文功能重新叠加到官方最新代码上
- `--force-with-lease` 用于安全推送 rebase 后的分支，避免覆盖远端其他人的新提交

## 处理 rebase 冲突

如果 `git rebase master` 出现冲突：

```bash
git status
```

逐个打开冲突文件，保留：

1. 官方新代码逻辑
2. 中文多语言接入逻辑
3. 新增英文 UI 文案对应的中文词条

解决完成后：

```bash
git add <冲突文件>
git rebase --continue
```

如果判断本次 rebase 方向错误，可以中止：

```bash
git rebase --abort
```

## 官方新增 UI 文案后的处理

官方更新可能新增英文 UI 文案。同步后需要检查这些新增文案是否已经接入中文翻译。

重点检查：

```text
app/src/settings_view/
app/src/search/
app/src/terminal/input/
app/src/workspace/
app/src/notebooks/
crates/warp_i18n/locales/en-US/ui_literals.ftl
crates/warp_i18n/locales/zh-CN/ui_literals.ftl
```

原则：

- 新增英文界面文案需要在 `en-US` 和 `zh-CN` 中保持一一对应
- 动态拼接文案优先放入 `crates/warp_i18n/src/lib.rs` 的动态翻译逻辑
- 基础 UI 文本入口继续复用现有 `Text`、`TuiText`、`Button`、`FormattedTextElement` 翻译链路

## 同步后验证

同步官方代码、解决冲突或修改翻译后，至少执行：

```bash
cargo fmt -p warp_i18n
cargo fmt -p warp -- --check
cargo check -p warp_i18n
cargo check -p warp --bin warp-oss --message-format short
git diff --check
```

如果需要重新打包 macOS App：

```bash
set -euo pipefail
mkdir -p /private/tmp/warp-home/.cache /private/tmp/warp-clang-module-cache /private/tmp/warp-cache
export HOME=/private/tmp/warp-home
export CARGO_HOME=/Users/baisensenseng/.cargo
export RUSTUP_HOME=/Users/baisensenseng/.rustup
export DEVELOPER_DIR=/Applications/Xcode-beta.app/Contents/Developer
export HOMEBREW_NO_AUTO_UPDATE=1
export RUNNER_NAME=nsc-local
export CLANG_MODULE_CACHE_PATH=/private/tmp/warp-clang-module-cache
export XDG_CACHE_HOME=/private/tmp/warp-cache
/private/tmp/warp_bundle_app_only.sh --channel oss --artifact app --nouniversal --arch aarch64 --nosign
```

## 推送规则

正常开发提交：

```bash
git push origin feature/i18n-zh-cn
```

rebase 之后推送：

```bash
git push --force-with-lease origin feature/i18n-zh-cn
```

不要使用：

```bash
git push upstream
```

## 当前基线

当前中文分支结构：

```text
feature/i18n-zh-cn
└── 中文多语言功能提交
    └── upstream/master 官方最新代码
```

以后只需要重复“同步官方代码 → rebase 中文分支 → 验证 → 推送到 origin”的流程。

## 参考资料

- GitHub Fork 文档：https://docs.github.com/articles/fork-a-repo
- GitHub 远程仓库文档：https://docs.github.com/en/get-started/git-basics/about-remote-repositories
- Git push 文档：https://git-scm.com/docs/git-push
- Git remote 文档：https://git-scm.com/docs/git-remote
