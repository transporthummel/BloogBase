import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";
import i18n from "../i18n";

// Tests assert on English copy regardless of the machine's locale.
await i18n.changeLanguage("en");

afterEach(cleanup);
