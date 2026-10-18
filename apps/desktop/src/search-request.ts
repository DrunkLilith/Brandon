/** Observe a request without allowing late results to update an abandoned UI. */
export function observeSearch<T>(request: Promise<T>, handlers: {
  result: (value: T) => void
  error: (error: unknown) => void
  complete: () => void
}): () => void {
  let active = true
  void request
    .then((value) => { if (active) handlers.result(value) })
    .catch((error: unknown) => { if (active) handlers.error(error) })
    .finally(() => { if (active) handlers.complete() })
  return () => { active = false }
}
