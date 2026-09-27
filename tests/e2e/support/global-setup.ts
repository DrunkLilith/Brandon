import { startDeployment } from './deployment'

export default async function globalSetup() {
  await startDeployment()
}
