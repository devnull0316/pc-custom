import { useEffect, useId, useRef, type KeyboardEvent, type ReactNode } from "react";

import { Icon } from "./Icon";

interface DialogProps {
  title: string;
  description: string;
  onClose: () => void;
  children: ReactNode;
  footer: ReactNode;
  width?: "normal" | "wide";
}

const FOCUSABLE_SELECTOR = [
  "button:not([disabled])",
  "[href]",
  "input:not([disabled])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  "[tabindex]:not([tabindex='-1'])",
].join(",");

export function Dialog({
  title,
  description,
  onClose,
  children,
  footer,
  width = "normal",
}: DialogProps) {
  const panelRef = useRef<HTMLDivElement>(null);
  const titleId = useId();

  useEffect(() => {
    const previous = document.activeElement instanceof HTMLElement
      ? document.activeElement
      : null;
    const frame = window.requestAnimationFrame(() => {
      const preferred = panelRef.current?.querySelector<HTMLElement>("[data-dialog-autofocus]");
      (preferred ?? panelRef.current?.querySelector<HTMLElement>(FOCUSABLE_SELECTOR))?.focus();
    });

    return () => {
      window.cancelAnimationFrame(frame);
      previous?.focus();
    };
  }, []);

  function handleKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
      return;
    }
    if (event.key !== "Tab") {
      return;
    }

    const focusable = Array.from(
      panelRef.current?.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR) ?? [],
    );
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (first === undefined || last === undefined) {
      event.preventDefault();
      return;
    }
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }

  return (
    <div className="dialog-backdrop" role="presentation">
      <div
        aria-describedby={`${titleId}-description`}
        aria-labelledby={titleId}
        aria-modal="true"
        className={`dialog-panel dialog-panel--${width}`}
        onKeyDown={handleKeyDown}
        ref={panelRef}
        role="dialog"
      >
        <header className="dialog-header">
          <div>
            <h2 id={titleId}>{title}</h2>
            <p id={`${titleId}-description`}>{description}</p>
          </div>
          <button aria-label="閉じる" className="icon-button" onClick={onClose} type="button">
            <Icon name="close" />
          </button>
        </header>
        <div className="dialog-body">{children}</div>
        <footer className="dialog-footer">{footer}</footer>
      </div>
    </div>
  );
}
