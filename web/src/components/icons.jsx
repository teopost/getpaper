import React from 'react';
import {
  Book,
  BookOpen,
  Home,
  Star,
  Code,
  Briefcase,
  Cloud,
  Coffee,
  Database,
  Flag,
  Globe,
  Heart,
  Lightbulb,
  Rocket,
  Settings,
  Users,
  Wrench,
  Calendar,
  Camera,
  Leaf,
  Palette,
  Shield,
  Zap,
  GraduationCap,
  Music,
  Map,
  Package,
  FileText,
  Lock,
} from 'lucide-react';

export const COLLECTION_ICONS = {
  collection: Book,
  book: BookOpen,
  home: Home,
  star: Star,
  code: Code,
  briefcase: Briefcase,
  cloud: Cloud,
  coffee: Coffee,
  database: Database,
  flag: Flag,
  globe: Globe,
  heart: Heart,
  lightbulb: Lightbulb,
  rocket: Rocket,
  settings: Settings,
  users: Users,
  tool: Wrench,
  calendar: Calendar,
  camera: Camera,
  leaf: Leaf,
  palette: Palette,
  shield: Shield,
  zap: Zap,
  academic: GraduationCap,
  music: Music,
  map: Map,
  package: Package,
  lock: Lock,
};

export const COLORS = [
  '#4E5C6E',
  '#0366d6',
  '#9E5CF7',
  '#FF825C',
  '#FF5C80',
  '#FFBE0B',
  '#42DED1',
  '#00D084',
  '#2F80ED',
  '#E25C5C',
  '#8B6F47',
  '#111319',
];

export function CollectionIcon({ collection, size = 18 }) {
  const Icon = COLLECTION_ICONS[collection?.icon] || Book;
  return <Icon size={size} color={collection?.color || '#4E5C6E'} strokeWidth={2.2} className="collection-icon" />;
}

export function DocIcon({ doc, size = 16 }) {
  if (doc?.emoji) return <span className="doc-emoji" style={{ fontSize: size - 1 }}>{doc.emoji}</span>;
  return <FileText size={size} className="doc-icon" />;
}

export const EMOJIS = (
  '📄 📝 📌 📎 📚 📖 📒 📕 📗 📘 📙 🗂️ 🗃️ 📁 📂 📊 📈 📉 🗓️ 📅 ✅ ☑️ ❗ ❓ ⚠️ 💡 🔥 ⭐ 🌟 ✨ 🎯 🚀 🛠️ 🔧 ⚙️ 🧪 🧭 🗺️ 🏁 🏆 🎉 🎨 🎵 🎬 📷 💻 🖥️ 📱 🔒 🔑 🛡️ 🐛 🧩 📦 🏷️ 💬 📣 📢 ✉️ 📬 🤝 👋 👥 👤 🧑‍💻 🏢 🏠 🌍 🌱 🌳 ☕ 🍕 💰 💳 🧾 ⏰ ⏳ 🔍 🔗 ❤️ 💙 💚 💛 💜 🖤 🤖 🧠 📐 🧮'
).split(' ');
