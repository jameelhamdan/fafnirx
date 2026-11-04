/**
 * Puts text on the clipboard. The app is often opened over plain `http://` on a home network, where the browser has no
 * `navigator.clipboard`; there the old way (select a hidden field and copy) still works. Resolves to whether it worked.
 */
export async function copyText(text: string): Promise<boolean> {
  try {
    if (navigator.clipboard && window.isSecureContext) {
      await navigator.clipboard.writeText(text)
      return true
    }
  } catch {
    // Refused (no permission, not focused): try the other way.
  }
  const field = document.createElement('textarea')
  field.value = text
  field.setAttribute('readonly', '')
  field.style.cssText = 'position:fixed;top:0;left:0;opacity:0'
  // Inside the open dialog: a dialog keeps the focus in itself, and a field outside it could not be selected.
  const host = document.activeElement?.closest('[role="dialog"]') ?? document.body
  host.appendChild(field)
  field.select()
  field.setSelectionRange(0, text.length)
  let copied = false
  try {
    copied = document.execCommand('copy')
  } catch {
    copied = false
  }
  field.remove()
  return copied
}
