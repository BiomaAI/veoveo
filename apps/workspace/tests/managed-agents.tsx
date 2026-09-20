// Explicit local HTTP fixtures; this entry is never part of a production bundle.
import React from "react";
import "../src/style.css";
import { createRoot } from "react-dom/client";
import { AgentManager } from "../../console/web/src/agent-management/AgentManager";
import { browserSession } from "../../console/web/src/csrf";

browserSession.csrfToken = "fixture-csrf";
createRoot(document.getElementById("root")!).render(<AgentManager app="workspace"/>);
