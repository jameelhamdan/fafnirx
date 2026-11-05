export const REPO = 'https://github.com/jameelhamdan/fafnirx'

/** A page of the documentation on GitHub, optionally at a heading. */
export const docs = (page: string, heading?: string) => `${REPO}/blob/master/docs/${page}.md${heading ? `#${heading}` : ''}`

const enc = encodeURIComponent

/** The Files page at a device (`<user>/<name>`, `master:<name>`) and a path in it, or with `file` of it open in the viewer. */
export const filesLink = (device: string, path = '', file?: string) =>
  `/files?device=${enc(device)}${path ? `&path=${enc(path)}` : ''}${file ? `&file=${enc(file)}` : ''}`

/** The Files page at a folder as clients name it: `<owner>/<name>`, or `master:<path>` for a folder of the master files. */
export const folderLink = (folder: string) => filesLink(folder)
