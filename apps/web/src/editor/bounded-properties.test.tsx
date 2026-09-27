import {cleanup,render,screen} from '@testing-library/react'
import {expect,it} from 'vitest'
import {MilkdownAdapter} from './milkdown-adapter'

it('bounded properties: fixed-seed hostile Markdown renders no active content',async()=>{
  let seed=0x51eed
  const next=()=>{seed^=seed<<13;seed^=seed>>>17;seed^=seed<<5;return seed>>>0}
  const schemes=['javascript:','JaVaScRiPt:','data:text/html,','vbscript:','java\nscript:','javascript&#58;','javascript%3A']
  for(let index=0;index<32;index++) {
    const scheme=schemes[next()%schemes.length]
    const markdown=`# Case ${index}\n\n<script>alert(1)</script>\n\n<img src=x onerror=alert(1)>\n\n[link](${scheme}alert%281%29)\n\n![image](${scheme}payload)\n\n<iframe srcdoc="attack"></iframe>`
    render(<MilkdownAdapter value={markdown} onChange={()=>{}} readOnly/>)
    expect(await screen.findByRole('heading',{name:`Case ${index}`})).toBeVisible()
    expect(document.querySelector('script,iframe,object,embed,[onerror],[onclick]')).toBeNull()
    for(const element of document.querySelectorAll('[href],[src]')) {
      const target=element.getAttribute('href')??element.getAttribute('src')??''
      expect(target.replace(/[\u0000-\u0020]/g,'')).not.toMatch(/^(javascript|vbscript|data):/i)
    }
    cleanup()
  }
})
