import { configureBrowserApplication } from "./browserApp";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { QueryClientProvider } from "@tanstack/react-query";
import { App } from "./App";
import { CliPairingPage } from "./computers/CliPairingPage";
import { ConfirmProvider } from "./components/ConfirmDialog";
import { queryClient } from "./queryClient";
import { ThemeProvider } from "./ThemeProvider";
import { retireRecordingWorker } from "./retireRecordingWorker";
import "./styles.css";

configureBrowserApplication("console");

async function start() {
  try {
    if (await retireRecordingWorker()) return;
  } catch (error) {
    console.warn("Could not retire the old recording service worker", error);
  }
  createRoot(document.getElementById("root")!).render(
    <StrictMode>
      <ThemeProvider>
        <QueryClientProvider client={queryClient}>
          <ConfirmProvider>
            {window.location.pathname.startsWith("/console/computers/") ? <CliPairingPage /> : <App />}
          </ConfirmProvider>
        </QueryClientProvider>
      </ThemeProvider>
    </StrictMode>
  );
}

void start();
