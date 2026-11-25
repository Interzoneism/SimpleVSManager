import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import App from "./App";
import { ContextMenuProvider } from "./components/ContextMenu";
import "./theme/theme.css";
import "./styles.css";

window.addEventListener("contextmenu", (event) => event.preventDefault(), { capture: true });

const queryClient = new QueryClient({
  defaultOptions: {
    // Data comes from the local backend, not a flaky network: no refetch-on-focus churn.
    queries: { refetchOnWindowFocus: false, retry: false, staleTime: Infinity },
  },
});

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <ContextMenuProvider><App /></ContextMenuProvider>
    </QueryClientProvider>
  </React.StrictMode>,
);
