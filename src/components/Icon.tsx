import type { IconName } from "../model";

interface IconProps {
  name: IconName;
  size?: number;
  className?: string;
}

export function Icon({ name, size = 18, className }: IconProps) {
  return (
    <svg
      aria-hidden="true"
      className={className}
      fill="none"
      height={size}
      stroke="currentColor"
      strokeLinecap="round"
      strokeLinejoin="round"
      strokeWidth="1.7"
      viewBox="0 0 24 24"
      width={size}
    >
      {name === "home" ? <><path d="M3.8 10.4 12 3.7l8.2 6.7" /><path d="M5.8 9.3v10.4h12.4V9.3M9.5 19.7v-6.2h5v6.2" /></> : null}
      {name === "action" ? <><path d="M4 6.5h10M4 12h16M4 17.5h8" /><circle cx="17" cy="6.5" r="2" /><circle cx="15" cy="17.5" r="2" /></> : null}
      {name === "timeline" ? <><path d="M7 4v16M7 7h10M7 12h7M7 17h9" /><circle cx="7" cy="7" r="1.6" /><circle cx="7" cy="12" r="1.6" /><circle cx="7" cy="17" r="1.6" /></> : null}
      {name === "search" ? <><circle cx="10.5" cy="10.5" r="6.2" /><path d="m15.1 15.1 4.6 4.6" /></> : null}
      {name === "game" ? <><path d="M7.2 8.2h9.6c2 0 3.2 1.4 3.5 3.3l.8 5.2c.3 2.2-2.3 3.5-3.7 1.8l-2-2.4H8.6l-2 2.4c-1.4 1.7-4 .4-3.7-1.8l.8-5.2c.3-1.9 1.5-3.3 3.5-3.3Z" /><path d="M7 11.2v3.4M5.3 12.9h3.4" /><circle cx="16.1" cy="12" fill="currentColor" r=".7" stroke="none" /><circle cx="18" cy="14" fill="currentColor" r=".7" stroke="none" /></> : null}
      {name === "appearance" ? <><circle cx="12" cy="12" r="8.5" /><path d="M12 3.5a8.5 8.5 0 0 1 0 17Z" fill="currentColor" opacity=".16" /><path d="M12 3.5v17" /></> : null}
      {name === "explorer" ? <><path d="M3.5 7.3h17v11.5a1.7 1.7 0 0 1-1.7 1.7H5.2a1.7 1.7 0 0 1-1.7-1.7Z" /><path d="M3.5 7.3V5.5a2 2 0 0 1 2-2h4l2 2h7a2 2 0 0 1 2 1.8" /></> : null}
      {name === "focus" ? <><path d="M8 3.5H5.5a2 2 0 0 0-2 2V8M16 3.5h2.5a2 2 0 0 1 2 2V8M8 20.5H5.5a2 2 0 0 1-2-2V16M16 20.5h2.5a2 2 0 0 0 2-2V16" /><circle cx="12" cy="12" r="3.2" /></> : null}
      {name === "power" ? <><path d="M12 2.8v8.3" /><path d="M7.1 5.8a8 8 0 1 0 9.8 0" /></> : null}
      {name === "recovery" || name === "undo" ? <><path d="m7.8 8.4-4.2.2.2-4.2" /><path d="M4.1 8.3A8.4 8.4 0 1 1 5 17" /></> : null}
      {name === "sleep" ? <><path d="M16.4 4.1a8.3 8.3 0 1 0 3.5 11.5A7.3 7.3 0 0 1 16.4 4Z" /><path d="M6.3 5.2h3.5L6.3 9.4h3.5" /></> : null}
      {name === "check" ? <path d="m4.5 12.5 4.5 4.4L19.5 6.6" /> : null}
      {name === "warning" ? <><path d="M10.4 4.4 2.8 18a1.3 1.3 0 0 0 1.1 2h16.2a1.3 1.3 0 0 0 1.1-2L13.6 4.4a1.8 1.8 0 0 0-3.2 0Z" /><path d="M12 9v4.6M12 17.2v.1" /></> : null}
      {name === "info" ? <><circle cx="12" cy="12" r="9" /><path d="M12 10.7v5.8M12 7.3v.1" /></> : null}
      {name === "arrow" ? <path d="M5 12h14m-5-5 5 5-5 5" /> : null}
      {name === "plus" ? <path d="M12 5v14M5 12h14" /> : null}
      {name === "close" ? <path d="m6 6 12 12M18 6 6 18" /> : null}
      {name === "spinner" ? <path className="spinner-path" d="M20 12a8 8 0 1 1-2.3-5.7" /> : null}
      {name === "chevron" ? <path d="m9 6 6 6-6 6" /> : null}
    </svg>
  );
}
