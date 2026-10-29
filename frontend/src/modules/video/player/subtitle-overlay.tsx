import { cueNodes } from './cue-text'
import { lineStyle, type SubtitleStyle } from './subtitle-style'

/**
 * The subtitles over the picture, in the look the person chose. `height` is the picture's, which the size follows;
 * `lift` is how many pixels the controls cover at the bottom (0 while they are hidden), so that the text sits above them.
 */
export function SubtitleOverlay({ lines, style, height, lift }: { lines: string[]; style: SubtitleStyle; height: number; lift: number }) {
  if (lines.length === 0) return null
  return (
    <div
      className="pointer-events-none absolute inset-x-0 flex flex-col items-center gap-0.5 px-[5%] text-center transition-[bottom] duration-200"
      style={{ bottom: `calc(${style.position}% + ${lift}px)` }}
    >
      {lines.map((text, i) => (
        <span key={i} dir="auto" className="max-w-full rounded-[0.2em] px-[0.4em] py-[0.05em] leading-snug whitespace-pre-wrap" style={lineStyle(style, height)}>
          {cueNodes(text)}
        </span>
      ))}
    </div>
  )
}
