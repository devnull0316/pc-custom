import React from "react";
import ReactDOM from "react-dom/client";

import { App } from "./App";
import "./styles.css";

const root = document.getElementById("root");
if (root === null) throw new Error("PCカスタム root element was not found");
ReactDOM.createRoot(root).render(<React.StrictMode><App /></React.StrictMode>);
