import { clsx, type ClassValue } from 'clsx'
import { twMerge } from 'tailwind-merge'

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}

/** Gives a small control (a 16 px checkbox) a 24 px target without changing how it looks (WCAG 2.5.8). */
export const HIT_AREA = "relative after:absolute after:-inset-1 after:content-['']"
