import "@testing-library/jest-dom/vitest";
import { afterEach } from "vitest";
import { toast } from "../components/Toast";

// Toasts live in a module-level store; start each test with none on screen.
afterEach(() => toast.clear());
