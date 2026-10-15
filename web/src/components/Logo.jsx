import React from 'react';

/** GetPaper mark: a sheet of paper with a folded corner. The tile uses the current text colour. */
export function LogoMark({ size = 40, className = '' }) {
  return (
    <svg width={size} height={size} viewBox="0 0 64 64" className={`logo ${className}`} aria-hidden="true">
      <rect width="64" height="64" rx="14" fill="currentColor" />
      <path d="M21 13h15.5L45 21.5V48a3 3 0 0 1-3 3H21a3 3 0 0 1-3-3V16a3 3 0 0 1 3-3z" fill="var(--bg)" />
      <path d="M36.5 13v6.5a2 2 0 0 0 2 2H45z" fill="currentColor" opacity="0.35" />
      <rect x="24" y="28" width="15" height="3" rx="1.5" fill="currentColor" />
      <rect x="24" y="34" width="15" height="3" rx="1.5" fill="currentColor" />
      <rect x="24" y="40" width="9" height="3" rx="1.5" fill="currentColor" />
    </svg>
  );
}

/** Mark and name, for the sign-in pages and public documents */
export function LogoFull({ size = 40 }) {
  return (
    <span className="logo-full" style={{ fontSize: Math.round(size * 0.62) }}>
      <LogoMark size={size} />
      <span className="logo-wordmark">
        Get<span>Paper</span>
      </span>
    </span>
  );
}
