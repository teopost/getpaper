import React, { useCallback, useEffect, useRef, useState } from 'react';
import { useLocation, useNavigate } from 'react-router-dom';
import { Bell, CheckCheck } from 'lucide-react';
import { api, collectionUrl, docUrl, timeAgo } from '../api';
import { Avatar } from './ui';
import { t } from '../i18n';

/** Fired after notifications have been marked as read elsewhere (e.g. by opening the document) */
export const NOTIFICATIONS_CHANGED = 'getpaper:notifications-changed';

export function notificationsChanged() {
  window.dispatchEvent(new Event(NOTIFICATIONS_CHANGED));
}

const POLL_MS = 60 * 1000;

/** What happened, before the name of the document or collection */
function notificationText(n) {
  const name = n.actorName || t('Someone');
  switch (n.event) {
    case 'document_updated':
      return t('{name} updated', { name });
    case 'document_published':
      return t('{name} published', { name });
    case 'document_shared':
      return t('{name} shared with you', { name });
    case 'comment_created':
      return t('{name} commented on', { name });
    case 'comment_resolved':
      return t('{name} resolved a thread in', { name });
    case 'collection_added':
      return t('{name} gave you access to', { name });
    case 'invite_accepted':
      return t('{name} accepted your invitation and joined the workspace', { name });
    case 'group_mentioned_in_document':
      return t('{name} mentioned {group} in', { name, group: `@${n.groupName}` });
    default:
      return t('{name} mentioned you in', { name });
  }
}

/** Sidebar entry with the unread count; opens the list of notifications */
export default function Notifications() {
  const [items, setItems] = useState([]);
  const [unread, setUnread] = useState(0);
  const [open, setOpen] = useState(false);
  const [pos, setPos] = useState(null);
  const buttonRef = useRef(null);
  const panelRef = useRef(null);
  const location = useLocation();
  const navigate = useNavigate();

  const refresh = useCallback(() => {
    api('notifications.list', { limit: 30 })
      .then((d) => {
        setItems(d.notifications);
        setUnread(d.unread);
      })
      .catch(() => {});
  }, []);

  useEffect(refresh, [refresh, location.pathname]);

  useEffect(() => {
    const timer = setInterval(() => document.visibilityState === 'visible' && refresh(), POLL_MS);
    window.addEventListener(NOTIFICATIONS_CHANGED, refresh);
    return () => {
      clearInterval(timer);
      window.removeEventListener(NOTIFICATIONS_CHANGED, refresh);
    };
  }, [refresh]);

  useEffect(() => {
    if (!open) return undefined;
    const onDown = (e) => {
      if (!panelRef.current?.contains(e.target) && !buttonRef.current?.contains(e.target)) setOpen(false);
    };
    const onKey = (e) => e.key === 'Escape' && setOpen(false);
    document.addEventListener('mousedown', onDown);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('mousedown', onDown);
      document.removeEventListener('keydown', onKey);
    };
  }, [open]);

  const toggle = () => {
    if (!open) {
      const r = buttonRef.current.getBoundingClientRect();
      const width = Math.min(360, window.innerWidth - 16);
      setPos({ top: r.top, left: Math.max(8, Math.min(r.right + 8, window.innerWidth - width - 8)), width });
      refresh();
    }
    setOpen(!open);
  };

  const markAll = async () => {
    await api('notifications.markRead').catch(() => {});
    refresh();
  };

  const openItem = async (n) => {
    setOpen(false);
    if (!n.viewedAt) await api('notifications.markRead', { id: n.id }).catch(() => {});
    refresh();
    if (n.documentUrlId) navigate(docUrl({ title: n.documentTitle, urlId: n.documentUrlId }) + (n.commentId ? `?comment=${n.commentId}` : ''));
    else if (n.collectionUrlId) navigate(collectionUrl({ name: n.collectionName, urlId: n.collectionUrlId }));
    else if (n.event === 'invite_accepted') navigate('/settings/members');
  };

  return (
    <>
      <button ref={buttonRef} className={`sidebar-link ${open ? 'active' : ''}`} onClick={toggle}>
        <span className="sidebar-icon">
          <Bell size={18} />
        </span>
        <span className="sidebar-label">{t('Notifications')}</span>
        {unread > 0 && <span className="sidebar-badge notification-badge">{unread}</span>}
      </button>
      {open && pos && (
        <div ref={panelRef} className="notifications-panel" style={{ top: pos.top, left: pos.left, width: pos.width }}>
          <div className="notifications-header">
            <span>{t('Notifications')}</span>
            {unread > 0 && (
              <button className="notifications-mark-all" onClick={markAll} title={t('Mark all as read')}>
                <CheckCheck size={16} />
                {t('Mark all as read')}
              </button>
            )}
          </div>
          <div className="notifications-list">
            {items.length === 0 && <div className="notifications-empty muted">{t('No notifications yet')}</div>}
            {items.map((n) => (
              <button key={n.id} className={`notification ${n.viewedAt ? '' : 'unread'}`} onClick={() => openItem(n)}>
                <Avatar name={n.actorName || '?'} color={n.actorColor} size={28} />
                <span className="notification-body">
                  <span className="notification-text">
                    {notificationText(n)}{' '}
                    {n.documentUrlId && (
                      <strong>
                        {n.documentEmoji ? `${n.documentEmoji} ` : ''}
                        {n.documentTitle || t('Untitled')}
                      </strong>
                    )}
                    {n.collectionName && <strong>{n.collectionName}</strong>}
                  </span>
                  <span className="notification-time muted">{timeAgo(n.createdAt)}</span>
                </span>
                {!n.viewedAt && <span className="notification-dot" />}
              </button>
            ))}
          </div>
        </div>
      )}
    </>
  );
}
