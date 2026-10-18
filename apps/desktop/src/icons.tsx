import type { SVGProps } from 'react'

type IconProps = SVGProps<SVGSVGElement>

function IconBase({ children, ...props }: IconProps) {
  return (
    <svg aria-hidden="true" fill="none" height="18" viewBox="0 0 24 24" width="18" {...props}>
      {children}
    </svg>
  )
}

export function BrandIcon(props: IconProps) {
  return (
    <IconBase {...props} viewBox="0 0 28 28">
      <path d="M7.8 3.5h7.5c4.2 0 6.7 2 6.7 5.3 0 2.2-1.2 3.8-3.2 4.5 2.6.6 4.2 2.4 4.2 5.1 0 3.8-3 6.1-7.8 6.1H7.8c-1.5 0-2.8-1.3-2.8-2.8V6.3c0-1.5 1.3-2.8 2.8-2.8Z" fill="currentColor" />
      <path d="M11 8h4.2c1.3 0 2 .6 2 1.6s-.7 1.6-2 1.6H11V8Zm0 7.5h4.8c1.5 0 2.3.7 2.3 1.8s-.8 1.8-2.3 1.8H11v-3.6Z" fill="#162019" />
    </IconBase>
  )
}

export function FolderIcon(props: IconProps) { return <IconBase {...props}><path d="M3.5 6.5h6l2 2h9v10h-17v-12Z" stroke="currentColor" strokeWidth="1.6" strokeLinejoin="round" /></IconBase> }
export function FileIcon(props: IconProps) { return <IconBase {...props}><path d="M6 3.5h8l4 4v13H6v-17Z" stroke="currentColor" strokeWidth="1.6" strokeLinejoin="round" /><path d="M14 3.5v4h4" stroke="currentColor" strokeWidth="1.6" /></IconBase> }
export function SearchIcon(props: IconProps) { return <IconBase {...props}><circle cx="10.5" cy="10.5" r="5.5" stroke="currentColor" strokeWidth="1.7" /><path d="m15 15 4.5 4.5" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" /></IconBase> }
export function PlusIcon(props: IconProps) { return <IconBase {...props}><path d="M12 5v14M5 12h14" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" /></IconBase> }
export function SaveIcon(props: IconProps) { return <IconBase {...props}><path d="M5 4h12l2 2v14H5V4Z" stroke="currentColor" strokeWidth="1.6" strokeLinejoin="round" /><path d="M8 4v6h8V4M8.5 20v-6h7v6" stroke="currentColor" strokeWidth="1.6" /></IconBase> }
export function ChevronIcon(props: IconProps) { return <IconBase {...props}><path d="m8 10 4 4 4-4" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" /></IconBase> }
export function SidebarIcon(props: IconProps) { return <IconBase {...props}><rect x="3.5" y="4" width="17" height="16" rx="2" stroke="currentColor" strokeWidth="1.6" /><path d="M9 4v16" stroke="currentColor" strokeWidth="1.6" /></IconBase> }
export function EditIcon(props: IconProps) { return <IconBase {...props}><path d="m5 17.5-.5 3 3-.5L18.8 8.7l-2.5-2.5L5 17.5Z" stroke="currentColor" strokeWidth="1.6" strokeLinejoin="round" /><path d="m14.8 7.7 2.5 2.5" stroke="currentColor" strokeWidth="1.6" /></IconBase> }
export function ColumnsIcon(props: IconProps) { return <IconBase {...props}><rect x="3.5" y="4" width="17" height="16" rx="2" stroke="currentColor" strokeWidth="1.6" /><path d="M12 4v16" stroke="currentColor" strokeWidth="1.6" /></IconBase> }
export function PreviewIcon(props: IconProps) { return <IconBase {...props}><path d="M2.5 12s3.4-6 9.5-6 9.5 6 9.5 6-3.4 6-9.5 6-9.5-6-9.5-6Z" stroke="currentColor" strokeWidth="1.6" /><circle cx="12" cy="12" r="2.5" stroke="currentColor" strokeWidth="1.6" /></IconBase> }
export function MoreIcon(props: IconProps) { return <IconBase {...props}><circle cx="5" cy="12" r="1.5" fill="currentColor" /><circle cx="12" cy="12" r="1.5" fill="currentColor" /><circle cx="19" cy="12" r="1.5" fill="currentColor" /></IconBase> }
export function MinimizeIcon(props: IconProps) { return <IconBase {...props}><path d="M6 12h12" stroke="currentColor" strokeWidth="1.4" /></IconBase> }
export function MaximizeIcon(props: IconProps) { return <IconBase {...props}><rect x="6" y="6" width="12" height="12" stroke="currentColor" strokeWidth="1.4" /></IconBase> }
export function CloseIcon(props: IconProps) { return <IconBase {...props}><path d="m7 7 10 10M17 7 7 17" stroke="currentColor" strokeWidth="1.4" /></IconBase> }
export function RefreshIcon(props: IconProps) { return <IconBase {...props}><path d="M19 8a7.5 7.5 0 1 0 .3 7.5" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" /><path d="M19 4v4h-4" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" /></IconBase> }
export function GraphIcon(props: IconProps) { return <IconBase {...props}><circle cx="6" cy="6" r="2.2" stroke="currentColor" strokeWidth="1.6" /><circle cx="18" cy="8" r="2.2" stroke="currentColor" strokeWidth="1.6" /><circle cx="10" cy="18" r="2.2" stroke="currentColor" strokeWidth="1.6" /><path d="m8 6.4 7.8 1.2M7.2 8l2 7.8m2.8.6 4.5-6.6" stroke="currentColor" strokeWidth="1.4" /></IconBase> }
export function CanvasIcon(props: IconProps) { return <IconBase {...props}><rect x="4" y="4" width="6" height="6" rx="1" stroke="currentColor" strokeWidth="1.5" /><rect x="14" y="4" width="6" height="6" rx="1" stroke="currentColor" strokeWidth="1.5" /><rect x="4" y="14" width="6" height="6" rx="1" stroke="currentColor" strokeWidth="1.5" /><rect x="14" y="14" width="6" height="6" rx="1" stroke="currentColor" strokeWidth="1.5" /></IconBase> }
export function TemplateIcon(props: IconProps) { return <IconBase {...props}><path d="M5 5.5A2.5 2.5 0 0 1 7.5 3H18v15.5H7.5A2.5 2.5 0 0 0 5 21V5.5Z" stroke="currentColor" strokeWidth="1.5" strokeLinejoin="round" /><path d="M5 18.5A2.5 2.5 0 0 1 7.5 16H18M9 7h5M9 10h4" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" /></IconBase> }
export function CalendarIcon(props: IconProps) { return <IconBase {...props}><rect x="3.5" y="5" width="17" height="15" rx="2" stroke="currentColor" strokeWidth="1.5" /><path d="M7.5 3v4M16.5 3v4M3.5 9h17M8 13h2M14 13h2M8 17h2" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" /></IconBase> }
export function DatabaseIcon(props: IconProps) { return <IconBase {...props}><ellipse cx="12" cy="5.5" rx="7.5" ry="3" stroke="currentColor" strokeWidth="1.5" /><path d="M4.5 5.5v6c0 1.7 3.4 3 7.5 3s7.5-1.3 7.5-3v-6M4.5 11.5v6c0 1.7 3.4 3 7.5 3s7.5-1.3 7.5-3v-6" stroke="currentColor" strokeWidth="1.5" /></IconBase> }
export function FitIcon(props: IconProps) { return <IconBase {...props}><path d="M8 4H4v4M16 4h4v4M20 16v4h-4M4 16v4h4" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" /></IconBase> }
export function LinkIcon(props: IconProps) { return <IconBase {...props}><path d="m9.5 14.5 5-5M8 17H6.5a4 4 0 0 1 0-8H9m6-2h2.5a4 4 0 0 1 0 8H15" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" /></IconBase> }
export function SettingsIcon(props: IconProps) { return <IconBase {...props}><path d="m9 3-.7 2.4-2 .9L4 5.8 2.5 8.4l1.8 1.7v2.4L2.5 14 4 16.7l2.3-.5 2 .9L9 20h3l.7-2.9 2-.9 2.3.5 1.5-2.7-1.8-1.5v-2.4l1.8-1.7L17 5.8l-2.3.5-2-.9L12 3H9Z" stroke="currentColor" strokeWidth="1.5" strokeLinejoin="round" /><circle cx="10.5" cy="11.5" r="3" stroke="currentColor" strokeWidth="1.5" /></IconBase> }
