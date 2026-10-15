import React, { useEffect } from 'react';
import { getProductName } from '../prefs';

/** Page shell with a sticky header (title/breadcrumb on the left, actions on the right). */
export default function Scene({ title, headerTitle, actions, children, wide, centered = true }) {
  useEffect(() => {
    if (typeof title === 'string') document.title = `${title} - ${getProductName()}`;
  }, [title]);
  return (
    <div className="scene">
      <header className="scene-header">
        <div className="scene-header-title">{headerTitle}</div>
        <div className="scene-header-actions">{actions}</div>
      </header>
      <div className={`scene-content ${wide ? 'wide' : ''} ${centered ? 'centered' : ''}`}>{children}</div>
    </div>
  );
}
