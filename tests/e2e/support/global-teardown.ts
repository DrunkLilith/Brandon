import { cleanupArtifacts } from './artifacts'
import { cleanupDeployment, readRuntimeStateIfPresent } from './deployment'

export default async function globalTeardown() {
  const state = readRuntimeStateIfPresent()
  try {
    if (state) cleanupDeployment(state)
  } finally {
    cleanupArtifacts()
  }
}
