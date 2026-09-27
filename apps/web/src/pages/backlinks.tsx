import {useQuery} from '@tanstack/react-query'
import {Link} from 'react-router-dom'
import {apiClient,unwrapArray} from '../api/client'
import {ErrorNotice,LoadingBlock} from '../admin/shared'
import {useI18n} from '../i18n'
import {pageQuery} from './queries'

function Backlink({id}:{id:string}) {
  const page=useQuery(pageQuery(id))
  return <li>{page.isPending?<LoadingBlock/>:page.isError?<ErrorNotice error={page.error}/>:<Link to={`/app/spaces/${page.data.spaceId}/pages/${id}`}>{page.data.title}</Link>}</li>
}
export function Backlinks({pageId}:{pageId:string}) {
  const {messages:text}=useI18n()
  const query=useQuery({queryKey:['workspace','backlinks',pageId],queryFn:async()=>unwrapArray(await apiClient.GET('/api/v1/pages/{id}/backlinks',{params:{path:{id:pageId}}}))})
  return <section className="admin-panel"><h2>{text.workspace.backlinks}</h2>{query.isPending?<LoadingBlock/>:query.isError?<ErrorNotice error={query.error}/>:query.data.length?<ul>{query.data.map(({id})=><Backlink key={id} id={id}/>)}</ul>:<p>{text.workspace.noBacklinks}</p>}</section>
}
