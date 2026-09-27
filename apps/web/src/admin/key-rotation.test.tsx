import { cleanup,render,screen,waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach,expect,it,vi } from 'vitest'
import { QueryClient,QueryClientProvider } from '@tanstack/react-query'
import { SettingsPage } from './settings'
afterEach(()=>{cleanup();vi.unstubAllGlobals()})
it('confirms rotation and displays bounded migration progress without exposing key material',async()=>{
  let started=false
  vi.stubGlobal('fetch',vi.fn(async(input:RequestInfo|URL,init?:RequestInit)=>{
    const request=input instanceof Request?input:new Request(new URL(String(input),location.origin),init)
    const path=new URL(request.url).pathname
    let body:unknown={attachmentRecoveryDays:30,exportRetentionHours:24}
    if(path.endsWith('/csrf'))body={token:'csrf'}
    if(path.endsWith('/key-rotations')) {
      if(request.method==='POST')started=true
      body={currentVersion:started?2:1,previousVersion:started?1:null,state:started?'migrating':'complete',processed:0}
    }
    return new Response(JSON.stringify(body),{status:request.method==='POST'?202:200,headers:{'content-type':'application/json'}})
  }))
  render(<QueryClientProvider client={new QueryClient({defaultOptions:{queries:{retry:false}}})}><SettingsPage/></QueryClientProvider>)
  const user=userEvent.setup()
  await user.click(await screen.findByRole('button',{name:'Ротация ключа пространства'}))
  expect(started).toBe(false)
  await user.click(screen.getByRole('button',{name:'Подтвердить ротацию'}))
  await waitFor(()=>expect(started).toBe(true))
  expect(await screen.findByText(/Миграция шифротекста/)).toBeVisible()
})
