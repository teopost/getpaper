import React from 'react';
import { EMOJIS } from './icons';
import { t } from '../i18n';

export default function EmojiPicker({ onSelect, onRemove }) {
  return (
    <div className="emoji-picker">
      <div className="emoji-grid">
        {EMOJIS.map((e) => (
          <button key={e} className="emoji-btn" onClick={() => onSelect(e)}>
            {e}
          </button>
        ))}
      </div>
      {onRemove && (
        <button className="emoji-remove" onClick={onRemove}>
          {t('Remove icon')}
        </button>
      )}
    </div>
  );
}
