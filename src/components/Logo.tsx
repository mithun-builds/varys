/**
 * Varys "whisper network" mark — 8 dots arranged around a centred eye, with
 * faint connecting lines. Uses currentColor so the parent can tint it
 * (white in the sidebar, accent-amber in the onboarding header).
 */
export function VarysLogo({ className, size = 28 }: { className?: string; size?: number }) {
  // Coordinates derived from icons/src/01_symbol_whisper_network_dark.svg —
  // a 200×200 viewport scaled to whatever pixel size the consumer asks for.
  return (
    <svg
      className={className}
      viewBox="0 0 200 200"
      width={size}
      height={size}
      xmlns="http://www.w3.org/2000/svg"
    >
      <g transform="translate(100, 100)">
        <g fill="currentColor" stroke="none">
          <circle cx="70" cy="0" r="6" />
          <circle cx="49.5" cy="-49.5" r="6" />
          <circle cx="0" cy="-70" r="6" />
          <circle cx="-49.5" cy="-49.5" r="6" />
          <circle cx="-70" cy="0" r="6" />
          <circle cx="-49.5" cy="49.5" r="6" />
          <circle cx="0" cy="70" r="6" />
          <circle cx="49.5" cy="49.5" r="6" />
        </g>
        <g stroke="currentColor" strokeWidth="2" fill="none" opacity="0.55">
          <line x1="70" y1="0" x2="14" y2="0" />
          <line x1="49.5" y1="-49.5" x2="9.9" y2="-9.9" />
          <line x1="0" y1="-70" x2="0" y2="-14" />
          <line x1="-49.5" y1="-49.5" x2="-9.9" y2="-9.9" />
          <line x1="-70" y1="0" x2="-14" y2="0" />
          <line x1="-49.5" y1="49.5" x2="-9.9" y2="9.9" />
          <line x1="0" y1="70" x2="0" y2="14" />
          <line x1="49.5" y1="49.5" x2="9.9" y2="9.9" />
        </g>
        <circle cx="0" cy="0" r="11" fill="none" stroke="currentColor" strokeWidth="3" />
        <circle cx="0" cy="0" r="5" fill="currentColor" />
      </g>
    </svg>
  );
}
