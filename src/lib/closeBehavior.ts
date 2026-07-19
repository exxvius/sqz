import { useEffect, useState } from "react";

/** What happens when the window's close button is pressed. */
export type CloseBehavior = "quit" | "tray";

const KEY = "sqz-close-behavior";

export function useCloseBehavior(): [CloseBehavior, (b: CloseBehavior) => void] {
  const [behavior, setBehavior] = useState<CloseBehavior>(() => {
    return localStorage.getItem(KEY) === "tray" ? "tray" : "quit";
  });

  useEffect(() => {
    localStorage.setItem(KEY, behavior);
  }, [behavior]);

  return [behavior, setBehavior];
}
