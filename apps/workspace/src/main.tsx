import { configureBrowserApplication } from "../../console/web/src/browserApp.ts";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import "@fontsource-variable/geist";
import App from "./App.tsx";
import "./style.css";

configureBrowserApplication("workspace");

const client = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: 10_000, gcTime: 0 } } });
createRoot(document.getElementById("root")!).render(<QueryClientProvider client={client}><App/></QueryClientProvider>);
