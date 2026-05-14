import { MemoryRouter, Routes, Route, Navigate } from "react-router-dom";
import "./App.css";
import Settings from "./routes/Settings";
import LiveSession from "./routes/LiveSession";

export default function App() {
  return (
    <MemoryRouter initialEntries={["/settings"]}>
      <Routes>
        <Route path="/settings" element={<Settings />} />
        <Route path="/live" element={<LiveSession />} />
        <Route path="*" element={<Navigate to="/settings" replace />} />
      </Routes>
    </MemoryRouter>
  );
}
