import { clsx, type ClassValue } from 'clsx'
import { extendTailwindMerge } from 'tailwind-merge'

/** `text-2xs` and `text-compact` are font sizes (theme.css): without this they would be taken for colours and not replace `text-sm`. */
const twMerge = extendTailwindMerge({ extend: { classGroups: { 'font-size': [{ text: ['2xs', 'compact'] }] } } })

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}

/** Gives a small control (a 16 px checkbox) a 24 px target without changing how it looks (WCAG 2.5.8). */
export const HIT_AREA = "relative after:absolute after:-inset-1 after:content-['']"
