export const REPO = 'https://github.com/jameelhamdan/fafnirx'

/** A page of the documentation on GitHub, optionally at a heading. */
export const docs = (page: string, heading?: string) => `${REPO}/blob/master/docs/${page}.md${heading ? `#${heading}` : ''}`

const enc = encodeURIComponent

/** The Files page at a device (`<user>/<name>`, `_master`) and a path in it, or with `file` of it open in the viewer. */
export const filesLink = (device: string, path = '', file?: string) =>
  `/files?device=${enc(device)}${path ? `&path=${enc(path)}` : ''}${file ? `&file=${enc(file)}` : ''}`

/** The Files page at a folder as clients name it: `<owner>/<name>`, or `master:<path>` for a folder of the master files. */
export const folderLink = (folder: string) => (folder.startsWith('master:') ? filesLink('_master', folder.slice(7)) : filesLink(folder))
