# Sola 模态输入系统计划 (Phase 5 - 最终完善)

## 1. 目标
目前的 Sola 在进行“新建文件”、“新建文件夹”和“重命名”操作时，使用的是硬编码的占位符（如 `untitled.md`）。为了提供完整的文件管理体验，我们需要在应用内构建一套极简的“模态输入框（Modal Input）”系统，允许用户自由输入名称。

## 2. 核心架构设计

### A. 输入状态管理 (`ProjectPanel`)
在 `ProjectPanel` 结构体中新增对输入模态框的状态追踪：
```rust
enum InputAction {
    CreateFile(PathBuf),
    CreateDir(PathBuf),
    Rename(PathBuf),
}

struct InputState {
    action: InputAction,
    value: String,
}
```

### B. 键盘事件拦截器 (Input Interceptor)
因为我们不需要复杂的富文本编辑能力，所以可以通过简单的状态机和键盘事件监听（`on_key_down`）来手写一个输入组件：
- **可打印字符**：监听按键字符并追加到 `value` 末尾。
- **Backspace**：删除 `value` 的最后一个字符。
- **Enter**：确认输入。根据 `InputAction` 的类型，调用 `workspace` 对应的 IO 方法，随后清空 `input_state`。
- **Esc**：取消输入，清空 `input_state`。

### C. 沉浸式 UI 渲染 (Modal Overlay)
- **遮罩层 (Mask)**：渲染一层半透明的黑色遮罩，覆盖整个 `ProjectPanel`（或全屏），防止用户在输入期间进行其他误操作。
- **输入框 (Input Box)**：在遮罩层中央悬浮一个高亮样式的弹窗，显示当前的提示语（如 "New File Name:"）以及用户正在输入的 `value`。追加一个闪烁的光标（Caret）提供反馈。

## 3. 实施步骤
1. **定义状态结构**：在 `project_panel.rs` 引入 `InputAction` 和 `InputState`。
2. **重构触发逻辑**：将右键菜单中的 IO 调用替换为唤起 `input_state`。
3. **实现渲染器**：编写 `render_input_modal` 方法。
4. **绑定键鼠事件**：在渲染出的遮罩上绑定全局键盘监听，完成字符捕获和 IO 闭环。
5. **验证**：测试右键 -> New File -> 输入名称 -> Enter 的连贯体验，确保文件树能正确渲染出新命名的文件。
