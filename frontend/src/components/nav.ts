import { Archive, Cog, FolderOpen, GalleryVerticalEnd, HardDrive, Heart, Images, LayoutDashboard, Monitor, Settings, Trash2, UploadCloud, Users } from 'lucide-react'
import { msg } from '@/lib/i18n'
import { hasWorkspace } from '@/lib/session'
import type { Role } from '@/lib/types'

/** Who sees an entry: everybody, the roles with a workspace of their own (not read-only), or administrators. */
export type Audience = 'all' | 'workspace' | 'admin'

export type NavItem = { to: string; label: string; icon: typeof Images; end?: boolean; for: Audience; photo?: true }

export const NAV: { title: string; items: NavItem[] }[] = [
  {
    title: msg('Photos'),
    items: [
      { to: '/timeline', label: msg('Timeline'), icon: Images, for: 'all', photo: true },
      { to: '/favorites', label: msg('Favorites'), icon: Heart, for: 'workspace', photo: true },
      { to: '/albums', label: msg('Albums'), icon: GalleryVerticalEnd, for: 'workspace', photo: true },
      { to: '/archive', label: msg('Archive'), icon: Archive, for: 'workspace', photo: true },
      { to: '/trash', label: msg('Trash'), icon: Trash2, for: 'workspace' },
    ],
  },
  {
    title: msg('Storage'),
    items: [
      { to: '/', label: msg('Dashboard'), icon: LayoutDashboard, end: true, for: 'all' },
      { to: '/files', label: msg('Files'), icon: FolderOpen, for: 'all' },
      { to: '/devices', label: msg('Devices & folders'), icon: Monitor, for: 'workspace' },
      { to: '/sources', label: msg('Sources'), icon: UploadCloud, for: 'workspace' },
      { to: '/queue', label: msg('Processing'), icon: Cog, for: 'all' },
      { to: '/mirrors', label: msg('Mirrors'), icon: HardDrive, for: 'admin' },
      { to: '/users', label: msg('Users'), icon: Users, for: 'admin' },
      { to: '/settings', label: msg('Settings'), icon: Settings, for: 'admin' },
    ],
  },
]

export const allowed = (audience: Audience, role: Role) => audience === 'all' || (audience === 'workspace' ? hasWorkspace(role) : role === 'admin')

/** The menu without the photo pages: what is left of their group (the trash) goes after Files in the storage group. */
export function withoutPhotos(nav: typeof NAV): typeof NAV {
  const [photos, storage] = nav
  const kept = photos.items.filter((i) => !i.photo)
  const at = storage.items.findIndex((i) => i.to === '/files') + 1
  return [{ ...storage, items: [...storage.items.slice(0, at), ...kept, ...storage.items.slice(at)] }]
}

/** The groups this person sees, with the entries they may open. */
export function visibleNav(role: Role, photos: boolean) {
  return (photos ? NAV : withoutPhotos(NAV))
    .map(({ title, items }) => ({ title, items: items.filter((i) => allowed(i.for, role)) }))
    .filter((g) => g.items.length > 0)
}
