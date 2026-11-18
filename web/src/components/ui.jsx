import React, { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { ChevronRight, X } from 'lucide-react';
import { t } from '../i18n';

export function Spinner({ size = 20 }) {
  return <div className="spinner" style={{ width: size, height: size }} />;
}

export function Button({ variant = 'primary', small, icon, children, className = '', ...props }) {
  return (
    <button className={`btn btn-${variant} ${small ? 'btn-small' : ''} ${className}`} {...props}>
      {icon}
      {children && <span>{children}</span>}
    </button>
  );
}

export function IconButton({ children, title, className = '', active, ...props }) {
  return (
    <button className={`icon-btn ${active ? 'active' : ''} ${className}`} title={title} aria-label={title} {...props}>
      {children}
    </button>
  );
}

export function Avatar({ name, color, size = 24 }) {
  const initial = (name || '?').trim().charAt(0).toUpperCase();
  return (
    <span
      className="avatar"
      style={{ width: size, height: size, background: color || '#4E5C6E', fontSize: Math.round(size * 0.45) }}
      title={name}
    >
      {initial}
    </span>
  );
}

export function TeamAvatar({ team, size = 24 }) {
  const initial = (team?.name || '?').trim().charAt(0).toUpperCase();
  if (team?.avatarUrl) return <img className="team-avatar team-logo" src={team.avatarUrl} alt="" style={{ width: size, height: size }} />;
  return (
    <span
      className="team-avatar"
      style={{ width: size, height: size, background: team?.color || '#0366d6', fontSize: Math.round(size * 0.5) }}
    >
      {initial}
    </span>
  );
}

export function Modal({ title, onClose, children, width = 480 }) {
  useEffect(() => {
    const onKey = (e) => e.key === 'Escape' && onClose?.();
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);
  return createPortal(
    <div className="modal-overlay" onMouseDown={(e) => e.target === e.currentTarget && onClose?.()}>
      <div className="modal" style={{ maxWidth: width }}>
        <div className="modal-header">
          <h2>{title}</h2>
          <IconButton title={t('Close')} onClick={onClose}>
            <X size={18} />
          </IconButton>
        </div>
        <div className="modal-body">{children}</div>
      </div>
    </div>,
    document.body
  );
}

/**
 * Dropdown menu. `items` is an array of { label, icon, onClick, danger, disabled, checked } or
 * { separator: true } or { heading: 'Title' }.
 */
export function Menu({ trigger, items, align = 'left', className = '', width = 220 }) {
  const [open, setOpen] = useState(false);
  const [pos, setPos] = useState(null);
  const triggerRef = useRef(null);
  const menuRef = useRef(null);
  // the open submenu: index of its item in the menu and where it is shown
  const [sub, setSub] = useState(null);
  const subRef = useRef(null);
  const subWidth = 200;

  useEffect(() => {
    if (!open) setSub(null);
  }, [open]);

  // next to its item, on the right (on the left when there is no room), inside the window
  const openSub = (index, el) => {
    const r = el.getBoundingClientRect();
    const m = menuRef.current?.getBoundingClientRect() || r;
    let left = m.right + 2;
    if (left + subWidth > window.innerWidth - 8) left = Math.max(8, m.left - subWidth - 2);
    setSub({ index, top: r.top - 6, left });
  };

  useLayoutEffect(() => {
    if (!sub || !subRef.current) return;
    const h = subRef.current.getBoundingClientRect().height;
    const top = Math.max(8, Math.min(sub.top, window.innerHeight - h - 8));
    if (top !== sub.top) setSub((s) => s && { ...s, top });
  }, [sub]);

  useLayoutEffect(() => {
    if (!open || !triggerRef.current) return;
    const r = triggerRef.current.getBoundingClientRect();
    let left = align === 'right' ? r.right - width : r.left;
    left = Math.max(8, Math.min(left, window.innerWidth - width - 8));
    let top = r.bottom + 4;
    setPos({ top, left });
  }, [open, align, width]);

  useLayoutEffect(() => {
    if (!open || !menuRef.current || !pos) return;
    const m = menuRef.current.getBoundingClientRect();
    if (m.bottom > window.innerHeight - 8) {
      const r = triggerRef.current.getBoundingClientRect();
      const top = Math.max(8, r.top - m.height - 4);
      if (top !== pos.top) setPos((p) => ({ ...p, top }));
    }
  }, [open, pos]);

  useEffect(() => {
    if (!open) return;
    const onDown = (e) => {
      if (menuRef.current?.contains(e.target) || subRef.current?.contains(e.target) || triggerRef.current?.contains(e.target)) return;
      setOpen(false);
    };
    const onKey = (e) => e.key === 'Escape' && setOpen(false);
    document.addEventListener('mousedown', onDown);
    window.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('mousedown', onDown);
      window.removeEventListener('keydown', onKey);
    };
  }, [open]);

  const list = typeof items === 'function' ? items() : items;
  const visible = open ? list.filter(Boolean) : [];

  const itemButton = (it, key, extra = {}) => (
    <button
      key={key}
      className={`menu-item ${it.danger ? 'danger' : ''}`}
      disabled={it.disabled}
      onClick={(e) => {
        e.stopPropagation();
        setOpen(false);
        it.onClick?.();
      }}
      {...extra}
    >
      {it.icon && <span className="menu-icon">{it.icon}</span>}
      <span className="menu-label">{it.label}</span>
      {it.checked && <span className="menu-check">✓</span>}
      {it.hint && <span className="menu-hint">{it.hint}</span>}
    </button>
  );
  const subItems = sub && visible[sub.index]?.submenu?.filter(Boolean);

  return (
    <>
      <span
        ref={triggerRef}
        className={`menu-trigger ${className}`}
        onClick={(e) => {
          e.preventDefault();
          e.stopPropagation();
          setOpen((o) => !o);
        }}
      >
        {trigger}
      </span>
      {open &&
        pos &&
        createPortal(
          <div className="menu" ref={menuRef} style={{ top: pos.top, left: pos.left, width }}>
            {visible.map((it, i) =>
              it.separator ? (
                <div key={i} className="menu-separator" onMouseEnter={() => setSub(null)} />
              ) : it.heading ? (
                <div key={i} className="menu-heading">
                  {it.heading}
                </div>
              ) : it.render ? (
                <div key={i}>{it.render(() => setOpen(false))}</div>
              ) : it.submenu ? (
                // an item that opens a list of choices beside the menu (on hover, or on click with touch)
                <button
                  key={i}
                  className={`menu-item has-submenu ${sub?.index === i ? 'open' : ''}`}
                  disabled={it.disabled}
                  aria-haspopup="menu"
                  aria-expanded={sub?.index === i}
                  onMouseEnter={(e) => openSub(i, e.currentTarget)}
                  onClick={(e) => {
                    e.stopPropagation();
                    if (sub?.index === i) setSub(null);
                    else openSub(i, e.currentTarget);
                  }}
                  onKeyDown={(e) => {
                    if (e.key === 'ArrowRight') {
                      openSub(i, e.currentTarget);
                      setTimeout(() => subRef.current?.querySelector('button')?.focus());
                    }
                  }}
                >
                  {it.icon && <span className="menu-icon">{it.icon}</span>}
                  <span className="menu-label">{it.label}</span>
                  <ChevronRight size={14} className="menu-submenu-arrow" />
                </button>
              ) : (
                itemButton(it, i, { onMouseEnter: () => setSub(null) })
              )
            )}
          </div>,
          document.body
        )}
      {open &&
        subItems?.length > 0 &&
        createPortal(
          <div
            className="menu menu-sub"
            role="menu"
            ref={subRef}
            style={{ top: sub.top, left: sub.left, width: subWidth }}
            onKeyDown={(e) => e.key === 'ArrowLeft' && setSub(null)}
          >
            {subItems.map((it, i) => itemButton(it, i))}
          </div>,
          document.body
        )}
    </>
  );
}

/** Generic popover anchored to a trigger, rendering arbitrary content. */
export function Popover({ trigger, children, align = 'right', width = 360, open: controlledOpen, onOpenChange }) {
  const [innerOpen, setInnerOpen] = useState(false);
  const open = controlledOpen ?? innerOpen;
  const setOpen = (v) => {
    setInnerOpen(v);
    onOpenChange?.(v);
  };
  const [pos, setPos] = useState(null);
  const triggerRef = useRef(null);
  const ref = useRef(null);
  useLayoutEffect(() => {
    if (!open || !triggerRef.current) return;
    const r = triggerRef.current.getBoundingClientRect();
    let left = align === 'right' ? r.right - width : r.left;
    left = Math.max(8, Math.min(left, window.innerWidth - width - 8));
    setPos({ top: r.bottom + 6, left });
  }, [open, align, width]);
  useEffect(() => {
    if (!open) return;
    const onDown = (e) => {
      if (ref.current?.contains(e.target) || triggerRef.current?.contains(e.target)) return;
      if (e.target.closest?.('.menu')) return;
      setOpen(false);
    };
    const onKey = (e) => e.key === 'Escape' && setOpen(false);
    document.addEventListener('mousedown', onDown);
    window.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('mousedown', onDown);
      window.removeEventListener('keydown', onKey);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);
  return (
    <>
      <span ref={triggerRef} className="menu-trigger" onClick={() => setOpen(!open)}>
        {trigger}
      </span>
      {open &&
        pos &&
        createPortal(
          <div className="popover" ref={ref} style={{ top: pos.top, left: pos.left, width }}>
            {typeof children === 'function' ? children(() => setOpen(false)) : children}
          </div>,
          document.body
        )}
    </>
  );
}

export function Switch({ checked, onChange, disabled }) {
  return (
    <button
      type="button"
      className={`switch ${checked ? 'on' : ''}`}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      role="switch"
      aria-checked={checked}
    >
      <span className="switch-knob" />
    </button>
  );
}

export function Tabs({ tabs, value, onChange }) {
  return (
    <div className="tabs">
      {tabs.map((tab) => (
        <button key={tab.value} className={`tab ${value === tab.value ? 'active' : ''}`} onClick={() => onChange(tab.value)}>
          {tab.label}
        </button>
      ))}
    </div>
  );
}

export function Empty({ children }) {
  return <div className="empty">{children}</div>;
}

/** Labelled form field. `group`: for a set of radio buttons or checkboxes, which carry their own labels. */
export function Field({ label, hint, group, children }) {
  const Tag = group ? 'div' : 'label';
  return (
    <Tag className="field" role={group ? 'group' : undefined}>
      {label && <span className="field-label">{label}</span>}
      {children}
      {hint && <span className="field-hint">{hint}</span>}
    </Tag>
  );
}

export function ConfirmModal({ title, message, confirmLabel = t('Confirm'), danger, onConfirm, onClose }) {
  const [busy, setBusy] = useState(false);
  return (
    <Modal title={title} onClose={onClose}>
      <p className="modal-text">{message}</p>
      <div className="modal-actions">
        <Button variant="secondary" onClick={onClose}>
          {t('Cancel')}
        </Button>
        <Button
          variant={danger ? 'danger' : 'primary'}
          disabled={busy}
          onClick={async () => {
            setBusy(true);
            try {
              await onConfirm();
              onClose();
            } finally {
              setBusy(false);
            }
          }}
        >
          {busy ? t('Working…') : confirmLabel}
        </Button>
      </div>
    </Modal>
  );
}
