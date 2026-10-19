# 贡献指南

感谢你愿意改进 Oddity。这个项目还在早期阶段，提交前请先阅读本页，能省掉很多来回沟通。

## 先看这些

- 使用与安装问题请移步至 [README](README.md)、[安装指南](docs/install.md) 和 [排障指南](docs/troubleshooting.md)。
- 安全漏洞不要提交公开 Issue，请按 [安全政策](SECURITY.md) 私下报告。
- 提交代码前，先在本地完成格式、测试和构建检查。

## 代码风格

### 注释

- **注释优先写中文**，尽量口语化。
- 不要用注释逐行翻译代码。代码本身能看懂的，不要再写一遍。
- 用户可见的输出要书面化，避免网络用语和随意的缩写，同时一律使用半角符号。
- 术语尽可能保持一致。

### Rust / 脚本 / 前端

- Rust 使用 `rustfmt` 默认配置：

  ```bash
  cargo fmt --all
  ```

- Rust 代码必须通过 Clippy：

  ```bash
  cargo clippy --workspace --all-targets --locked -- -D warnings
  ```

- Shell 脚本必须通过 ShellCheck。
- 前端目前是原生 HTML/CSS/ES modules，不做额外打包；改动后至少确认浏览器预览和 Tauri 窗口都能正常加载。
- 不要顺手格式化无关文件，也不要把重构和新功能混在同一个 PR 里。

## 测试与覆盖率

- 新增功能必须带测试；修 bug 时先补一个能复现问题的测试，再修代码。
- 测试要断言实际行为，不要只把代码跑一遍。
- 只有你的**总行覆盖率必须达到 80% 及以上。** CI 才会执行：

  ```bash
  cargo llvm-cov --workspace --locked --lcov --output-path lcov.info
  cargo llvm-cov report --fail-under-lines 80
  ```

- 不要通过排除文件、空测试、只调用不断言、修改门槛等方式绕过覆盖率要求。确实无法测试的代码，请在 PR 中说明原因，由维护者判断。

## 提交 PR 前先本地构建

请在 Open PR 之前，在自己的机器上完整构建一遍，不要只依赖 CI。

基础检查：

```bash
cd launcher

cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --release --locked -p nte-app -p nte-cli
```

仓库与打包检查：

```bash
node scripts/check-repo.mjs
node scripts/check-packaging.mjs
```

如果改动涉及 UI、桌面入口或打包，再构建一次 Debian 包：

```bash
npm install --global @tauri-apps/cli@2
cd launcher/crates/nte-app
tauri build --bundles deb --ci
```

至少运行与改动相关的检查，如果你不确定，那就把全部检查跑完。

## Commit 约定

Commit 标题使用模块前缀：

```text
module: change
```

例如：

```text
ui: 调整设置页的间距
core: 修复资源清单解析失败
ci: 添加覆盖率下限检查
docs: 补充首次启动说明
```

说明：

- 冒号使用半角 `:`，与现有提交历史和工具链保持一致。
- 如果一次提交包含多个变更，标题写影响最大的那个，其余变更放在提交描述里。
- 一次提交尽量只做一件相关的事，不要把格式化、重构和功能混在一起。
- 描述里写清楚改了什么、为什么改、有什么影响，不要只写 `fix bug` 或 `update`。

## AI 使用

可以使用 AI 写代码和文档，但必须自己先看一遍、跑一遍再提交。

- 不要让 AI 把文档当成代码编写，也不要只改几个词就当成人工写的说明。
- 提交前核对事实、命令、路径、链接和版本号是否真的正确。
- 不要伪造测试结果或覆盖率数据。
- 不要提交模型生成的、与本次改动无关的大段内容。

## PR 要求

### 不要破坏已有功能

PR 不能破坏当前已经存在的功能或内容。如果确实需要不兼容变更，必须做到：

- 在 PR 描述中明确写出影响范围、迁移方式和回滚方式；
- 在 PR 标题或描述中标注破坏性变更；
- 必要时先开 Issue 讨论，不要直接合并。

### PR 描述要有实质内容

PR 描述至少要说明：

1. 这个 PR 做了什么；
2. 为什么需要它；
3. 用户或开发者怎么使用这个改动；
4. 如何验证；
5. 是否有破坏性变更、副作用或已知限制；
6. 测试和覆盖率情况；
7. 涉及界面时附截图或录屏。

不要只写 `fix bug`、`update`、`优化一下`这种没有信息量的描述。

## 不要动这三个文件

`AGENTS.md`、`CLAUDE.md`、`GEMINI.md` 由维护者管理。

除非 Issue 或 PR 明确要求，否则不要修改、删除、重命名或**顺手整理**这三个文件。
也请 AI 工具也不要把它们当成可自由编辑的普通文档。

## 安全

发现安全问题时，请不要公开细节，按 [SECURITY.md](SECURITY.md) 中的方式私下报告。

---

提交前如果有不确定的地方，先开 Issue 问，比提交一个方向错误的 PR 更省时间。
