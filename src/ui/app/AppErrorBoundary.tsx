import {
  Component,
  type ErrorInfo,
  type PropsWithChildren,
  type ReactNode,
} from "react";

/** Error-boundary properties. */
export interface AppErrorBoundaryProps extends PropsWithChildren {
  /** Localized fallback content. */ fallback: ReactNode;
}
interface State {
  failed: boolean;
}

/** Prevents one rendering failure from blanking the entire application. */
export class AppErrorBoundary extends Component<AppErrorBoundaryProps, State> {
  public override state: State = { failed: false };
  public static getDerivedStateFromError(): State {
    return { failed: true };
  }
  public override componentDidCatch(error: Error, information: ErrorInfo) {
    console.error("PVLog UI boundary", {
      error: error.message,
      componentStack: information.componentStack,
    });
  }
  public override render() {
    return this.state.failed ? this.props.fallback : this.props.children;
  }
}
