import { useCallback, useEffect, useRef, useState } from "react";

const AUTO_HIDE_MS = 3000;

export function useReaderToolbar() {
  const [toolbarVisible, setToolbarVisible] = useState(false);
  const hideTimer = useRef<number | null>(null);

  const showToolbar = useCallback(() => {
    setToolbarVisible(true);
    if (hideTimer.current !== null) {
      window.clearTimeout(hideTimer.current);
    }
    hideTimer.current = window.setTimeout(() => {
      setToolbarVisible(false);
    }, AUTO_HIDE_MS);
  }, []);

  useEffect(() => {
    return () => {
      if (hideTimer.current !== null) {
        window.clearTimeout(hideTimer.current);
      }
    };
  }, []);

  return {
    toolbarVisible,
    showToolbar,
  };
}
