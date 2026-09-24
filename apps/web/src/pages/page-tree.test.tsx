import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, expect, it } from 'vitest'
import { PageTree } from './page-tree'
afterEach(cleanup)
const nodes = [
  {
    id: 'root',
    parentId: null,
    title: 'Root',
    sortKey: 'a',
    version: 1,
    restricted: false,
    capabilities: { edit: true, managePermissions: true },
  },
  {
    id: 'child',
    parentId: 'root',
    title: 'Child',
    sortKey: 'a',
    version: 1,
    restricted: false,
    capabilities: { edit: true, managePermissions: true },
  },
]
it('reveals a deep-link selection and synchronizes active keyboard focus after navigation', () => {
  const result = render(
    <PageTree nodes={nodes} selected="root" onSelect={() => {}} />,
  )
  const tree = screen.getByRole('tree')
  tree.focus()
  fireEvent.keyDown(tree, { key: 'ArrowLeft' })
  expect(screen.queryByRole('treeitem', { name: 'Child' })).toBeNull()
  result.rerender(
    <PageTree nodes={nodes} selected="child" onSelect={() => {}} />,
  )
  expect(screen.getByRole('treeitem', { name: 'Child' })).toHaveAttribute(
    'aria-selected',
    'true',
  )
  expect(tree).toHaveAttribute('aria-activedescendant', 'tree-child')
  expect(tree).toHaveFocus()
})
