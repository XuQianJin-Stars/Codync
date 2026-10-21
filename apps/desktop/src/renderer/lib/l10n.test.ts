import assert from 'node:assert/strict'
import { test } from 'node:test'
import { languageTitle, systemIsChinese, translate, usesChinese } from './l10n.ts'

test('chinese follows the choice, and a system preference follows the computer', () => {
  assert.equal(usesChinese('zh', false), true)
  assert.equal(usesChinese('en', true), false)
  assert.equal(usesChinese('system', true), true)
  assert.equal(usesChinese('system', false), false)
  assert.equal(systemIsChinese('zh-Hans-CN'), true)
  assert.equal(systemIsChinese('en-US'), false)
})

test('known chrome becomes Chinese and everything else stays', () => {
  assert.equal(translate('Pair a computer', 'zh', false), '配对一台电脑')
  assert.equal(translate('  Pair a computer\n', 'zh', false), '配对一台电脑')
  assert.equal(translate('Pair a computer', 'en', true), 'Pair a computer')
  assert.equal(translate('Not a phrase', 'system', true), 'Not a phrase')
  assert.equal(languageTitle('system', 'zh', false), '跟随系统')
  assert.equal(languageTitle('zh', 'en', true), '中文')
  assert.equal(languageTitle('en', 'zh', false), 'English')
})
