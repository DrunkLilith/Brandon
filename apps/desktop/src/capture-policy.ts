// Conservatively treat case aliases as the same portable target. The native
// preview also resolves an existing target to its canonical on-disk spelling.
export function sameCapturePath(left:string|undefined|null,right:string){return Boolean(left)&&left?.replaceAll('\\','/').toLowerCase()===right.replaceAll('\\','/').toLowerCase()}
