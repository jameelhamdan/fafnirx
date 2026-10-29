export const REPO = 'https://github.com/jameelhamdan/fafnirx'

/** A page of the documentation on GitHub, optionally at a heading. */
export const docs = (page: string, heading?: string) => `${REPO}/blob/master/docs/${page}.md${heading ? `#${heading}` : ''}`
