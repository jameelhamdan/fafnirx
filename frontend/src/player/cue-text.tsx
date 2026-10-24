import { createElement, Fragment, type ReactNode } from 'react'

type Piece = string | Marked
interface Marked {
  tag: 'i' | 'b' | 'u'
  kids: Piece[]
}

const ENTITIES: Record<string, string> = { amp: '&', lt: '<', gt: '>', quot: '"', apos: "'", nbsp: ' ', lrm: '‎', rlm: '‏' }
const decode = (text: string) =>
  text.replace(/&(#x[0-9a-f]+|#\d+|[a-z]+);/gi, (whole, name: string) => {
    if (name[0] === '#') {
      const code = name[1].toLowerCase() === 'x' ? parseInt(name.slice(2), 16) : parseInt(name.slice(1), 10)
      return Number.isFinite(code) && code > 0 && code <= 0x10ffff ? String.fromCodePoint(code) : whole
    }
    return ENTITIES[name.toLowerCase()] ?? whole
  })

function render(pieces: Piece[], key = ''): ReactNode[] {
  return pieces.map((piece, i) => {
    const id = `${key}${i}`
    if (typeof piece !== 'string') return createElement(piece.tag, { key: id }, render(piece.kids, `${id}.`))
    return createElement(
      Fragment,
      { key: id },
      ...piece.split('\n').flatMap((line, n) => (n ? [createElement('br', { key: n }), line] : [line])),
    )
  })
}

/**
 * The text of a WebVTT cue as elements. Only italic, bold and underline are kept (what subtitles use); every other tag
 * (voices, classes, timestamps) is dropped and nothing in the text is ever taken as markup, so a subtitle file cannot
 * put anything on the page.
 */
export function cueNodes(text: string): ReactNode[] {
  const root: Marked = { tag: 'i', kids: [] }
  const open: Marked[] = [root]
  for (const part of text.split(/(<[^>]*>)/)) {
    if (!part) continue
    if (part.startsWith('<') && part.endsWith('>')) {
      const closing = part[1] === '/'
      const name = part.slice(closing ? 2 : 1).split(/[\s.>]/)[0].toLowerCase()
      if (name !== 'i' && name !== 'b' && name !== 'u') continue
      if (closing) {
        if (open.length > 1 && open[open.length - 1].tag === name) open.pop()
      } else {
        const next: Marked = { tag: name, kids: [] }
        open[open.length - 1].kids.push(next)
        open.push(next)
      }
    } else {
      open[open.length - 1].kids.push(decode(part))
    }
  }
  return render(root.kids)
}
