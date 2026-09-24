import { workspaceMessages as text } from '../pages/messages'
// Linear space/time prefix/suffix comparison. The changed middle is intentionally coarse and bounded.
export function DiffView({ before, after }: { before: string; after: string }) {
  const left = before.slice(0, 100000).split('\n'),
    right = after.slice(0, 100000).split('\n')
  let prefix = 0,
    suffix = 0
  while (
    prefix < left.length &&
    prefix < right.length &&
    left[prefix] === right[prefix]
  )
    prefix++
  while (
    suffix < left.length - prefix &&
    suffix < right.length - prefix &&
    left[left.length - 1 - suffix] === right[right.length - 1 - suffix]
  )
    suffix++
  return (
    <section aria-label={text.workspace.diff}>
      {(before.length > 100000 || after.length > 100000) && (
        <p>{text.workspace.diffBound}</p>
      )}
      <pre>
        {left.slice(0, prefix).join('\n')}
        {'\n'}
        <del>{left.slice(prefix, left.length - suffix).join('\n')}</del>
        {'\n'}
        <ins>{right.slice(prefix, right.length - suffix).join('\n')}</ins>
        {'\n'}
        {suffix ? left.slice(-suffix).join('\n') : ''}
      </pre>
    </section>
  )
}
