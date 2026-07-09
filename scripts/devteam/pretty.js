#!/usr/bin/env node
// scripts/devteam/pretty.js — render Claude Code --output-format stream-json
// as a live progress feed. Reads JSONL on stdin, prints one line per event.
// Unknown/non-JSON lines are ignored (raw transcript still goes to the log).
//
//   claude -p "..." --output-format stream-json --verbose | tee -a log | node pretty.js worker
'use strict';

const label = process.argv[2] || 'agent';
const t0 = Date.now();
const clamp = (s, n) => { s = String(s ?? '').replace(/\s+/g, ' ').trim(); return s.length > n ? s.slice(0, n - 1) + '…' : s; };
const el = () => { const s = Math.floor((Date.now() - t0) / 1000); return String(Math.floor(s / 60)).padStart(2, '0') + ':' + String(s % 60).padStart(2, '0'); };
const out = (icon, msg) => process.stdout.write(`  [${label} ${el()}] ${icon} ${msg}\n`);

// what to show for each tool's primary argument
const arg = (name, i = {}) => {
  switch (name) {
    case 'Bash': return clamp(i.command, 88);
    case 'Read': case 'Edit': case 'Write': case 'NotebookEdit': return clamp(i.file_path, 78);
    case 'Grep': return clamp(`${i.pattern ?? ''} ${i.path ?? ''}`, 78);
    case 'Glob': return clamp(i.pattern, 78);
    case 'Task': return clamp(i.description, 78);
    case 'TodoWrite': return clamp((i.todos || []).filter(t => t.status === 'in_progress').map(t => t.content)[0] || `${(i.todos || []).length} todos`, 78);
    default: return clamp(i.description || i.file_path || i.command || i.pattern || '', 78);
  }
};
const icon = n => ({ Bash: '⚙', Read: '👁', Edit: '✎', Write: '✎', Grep: '🔍', Glob: '🔍', Task: '⇢', TodoWrite: '☑' }[n] || '🔧');

let toolsUsed = 0, edits = 0;

require('readline').createInterface({ input: process.stdin, crlfDelay: Infinity }).on('line', line => {
  let e;
  try { e = JSON.parse(line); } catch { return; }          // non-JSON → log only
  try {
    if (e.type === 'system' && e.subtype === 'init') {
      out('▸', `session started (model ${e.model ?? '?'}, cwd ${clamp(e.cwd, 40)})`);
    } else if (e.type === 'assistant') {
      for (const c of e.message?.content ?? []) {
        if (c.type === 'text') {
          const first = String(c.text || '').split('\n').map(s => s.trim()).find(Boolean);
          if (first) out('💬', clamp(first, 108));
        } else if (c.type === 'tool_use') {
          toolsUsed++;
          if (['Edit', 'Write'].includes(c.name)) edits++;
          out(icon(c.name), `${c.name}  ${arg(c.name, c.input)}`);
        }
      }
    } else if (e.type === 'user') {
      for (const c of e.message?.content ?? []) {
        if (c.type === 'tool_result' && c.is_error) out('✖', `tool error: ${clamp(typeof c.content === 'string' ? c.content : JSON.stringify(c.content), 90)}`);
      }
    } else if (e.type === 'result') {
      const secs = Math.round((e.duration_ms ?? 0) / 1000);
      const cost = e.total_cost_usd != null ? ` $${Number(e.total_cost_usd).toFixed(2)}` : '';
      out('──', `session end: ${e.subtype ?? 'ok'} · ${secs}s · ${e.num_turns ?? 0} turns · ${toolsUsed} tools · ${edits} edits${cost}`);
    }
  } catch { /* never let the pretty-printer kill the run */ }
});
