# Motion 类型声明修正

Motion for Vue 2.8.0 与现有 Vue 3.5、TypeScript 5.9 使用严格类型检查时, 需要以下声明修正.
pnpm 通过 `pnpm-workspace.yaml` 应用补丁, 原始版本和补丁哈希均由锁文件固定.

- `motion-v@2.8.0.patch`: 全局 HTML 属性保留 Vue 原生 `style` 和 `onDrag` 类型,
  Motion 组件自身仍使用其完整的动画与手势属性; `MaybeRef` 从 Vue 导入.
- `framer-motion@14.3.1.patch`: DOM 标签映射中的非标准 `webview` 使用 `HTMLElement`,
  避免要求普通浏览器环境提供 Electron 的 `HTMLWebViewElement`.

这些补丁仅修改 `.d.ts`, 不修改库运行时代码. 项目继续检查依赖类型声明, 没有启用 `skipLibCheck`.
VueUse 所需的 Web Bluetooth 类型通过开发依赖 `@types/web-bluetooth` 显式提供.

上游来源和许可证见仓库根目录的 [THIRD_PARTY.md](../../../THIRD_PARTY.md).
