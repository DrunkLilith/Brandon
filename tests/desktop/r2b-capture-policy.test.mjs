import assert from 'node:assert/strict'
import {test} from 'node:test'
import {sameCapturePath} from '../../apps/desktop/src/capture-policy.ts'
test('capture dirty-buffer guards include case/separator aliases without matching unrelated paths',()=>{assert.equal(sameCapturePath('Notes/Inbox.md','notes/INBOX.MD'),true);assert.equal(sameCapturePath('Notes\\Inbox.md','notes/Inbox.md'),true);assert.equal(sameCapturePath('Notes/Inbox.md','Inbox.md'),false);assert.equal(sameCapturePath(null,'Inbox.md'),false);assert.equal(sameCapturePath('Notes/Inbox.md','Notes/Inbox2.md'),false)})
