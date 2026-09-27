import { readRuntimeState } from './deployment'

export function endpoint(pathname: string) {
  return new URL(pathname, readRuntimeState().baseUrl).toString()
}
