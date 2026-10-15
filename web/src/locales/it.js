// Italian translations. Keys are the English strings used in the code (see src/i18n.js).
// Entries are merged from one module per area so they can be edited independently.
import common from './it/common';
import components from './it/components';
import document from './it/document';
import editor from './it/editor';
import pages from './it/pages';
import settings from './it/settings';

export default { ...components, ...document, ...editor, ...pages, ...settings, ...common };
