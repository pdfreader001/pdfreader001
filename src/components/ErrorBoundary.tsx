import { Component, type ErrorInfo, type ReactNode } from "react";
import { useApp } from "../state/store";

interface Props {
  children: ReactNode;
}

interface State {
  hasError: boolean;
  message: string;
}

/**
 * 全局 ErrorBoundary：捕获 React 组件树中未处理的 render 错误，
 * 避免整应用白屏。错误同时通过 toast 系统通知用户。
 *
 * 注意：Error Boundary 只捕获 render 阶段的同步错误。
 * - 异步操作（useEffect 回调、事件处理器、IPC Promise）的错误
 *   应在各自 try/catch 里用 errorToast(e) 主动上报。
 * - 本边界兜底防止"漏网"导致整个 React 树崩溃。
 */
export class ErrorBoundary extends Component<Props, State> {
  public state: State = { hasError: false, message: "" };

  public static getDerivedStateFromError(error: Error): State {
    return { hasError: true, message: error.message || String(error) };
  }

  public componentDidCatch(error: Error, _info: ErrorInfo): void {
    // 通知用户（通过 Zustand 静态调用，不依赖 hook）
    try {
      useApp.getState().pushToast("error", `组件错误：${error.message || String(error)}`);
    } catch {
      // store 本身也挂了 → 只能 console
      console.error("[ErrorBoundary]", error);
    }
  }

  private handleRetry = () => {
    this.setState({ hasError: false, message: "" });
  };

  public render(): ReactNode {
    if (this.state.hasError) {
      return (
        <div
          style={{
            display: "flex",
            flexDirection: "column",
            alignItems: "center",
            justifyContent: "center",
            height: "100vh",
            padding: 24,
            textAlign: "center",
            gap: 16,
          }}
        >
          <div style={{ fontSize: 48 }}>⚠️</div>
          <h2 style={{ margin: 0 }}>组件渲染异常</h2>
          <p
            style={{
              color: "var(--fg-dim)",
              maxWidth: 480,
              fontSize: 14,
              wordBreak: "break-all",
            }}
          >
            {this.state.message}
          </p>
          <div style={{ display: "flex", gap: 8 }}>
            <button onClick={this.handleRetry}>重试</button>
            <button onClick={() => location.reload()} className="btn-primary">
              刷新页面
            </button>
          </div>
        </div>
      );
    }
    return this.props.children;
  }
}

export default ErrorBoundary;
