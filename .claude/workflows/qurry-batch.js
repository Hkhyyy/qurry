export const meta = {
  name: 'qurry-batch',
  description: 'Qurry team harness: lead plans the batch, specialists build each item on its own branch, cross-review with adversarial checks, fix rounds, lead integrates and opens one PR',
  whenToUse: 'A batch of Qurry work (features, bugs, review findings) that should end as one verified PR. args: { request, repo, base?, batch?, pr?, items?, title?, parallel?, fixRounds?, sharedDbs?, targetDir? }',
  phases: [
    { title: 'Plan', detail: 'qurry-lead splits the request into work items with owners and checks' },
    { title: 'Build', detail: 'each item on its own branch, in its own worktree, by its owner' },
    { title: 'Review', detail: 'security / UI / perf lenses on each branch, findings challenged by a skeptic' },
    { title: 'Fix', detail: 'owner fixes confirmed findings, then re-review (max 2 rounds)' },
    { title: 'Integrate', detail: 'qurry-lead merges the branches, runs full verification, opens the PR' },
  ],
}

// ───────── inputs ─────────
const A = typeof args === 'string' ? { request: args } : args || {}
if (!A.request) throw new Error('args.request is required (what this batch should do)')
// Absolute path of the main checkout on this machine (it stays detached; agents work in worktrees under it)
if (!A.repo) throw new Error('args.repo is required (absolute path of the main checkout, e.g. the output of `git rev-parse --show-toplevel`)')
const REPO = A.repo.replace(/\/+$/, '')
const BASE = A.base || 'main'
const BATCH = A.batch || null // the lead picks one if empty
const OPEN_PR = A.pr !== false
const MAX_FIX_ROUNDS = A.fixRounds ?? 1
// Items worked on at the same time (each item runs build → reviews → fixes); keep low on a laptop
const PARALLEL = A.parallel ?? 3

const OWNER_TYPE = { 'qurry-security': 'qurry-security', 'qurry-ui': 'qurry-ui', 'qurry-perf': 'qurry-perf', implementer: 'general-purpose' }

// ───────── schemas ─────────
const PLAN = {
  type: 'object',
  properties: {
    batch: { type: 'string', description: 'batch branch name, e.g. feature/hardening' },
    title: { type: 'string', description: 'PR title, English, [feat]/[fix] prefix' },
    items: {
      type: 'array',
      items: {
        type: 'object',
        properties: {
          slug: { type: 'string' },
          owner: { type: 'string', enum: ['qurry-security', 'qurry-ui', 'qurry-perf', 'implementer'] },
          branch: { type: 'string' },
          task: { type: 'string', description: 'precise instructions for the owner' },
          files: { type: 'array', items: { type: 'string' } },
          verify: { type: 'array', items: { type: 'string' }, description: 'concrete checks that prove it works' },
          userVisible: { type: 'boolean', description: 'changes GUI/TUI behavior or looks' },
          runtime: { type: 'boolean', description: 'touches hot paths, async, memory, large data' },
          breaks: { type: 'string', description: 'who could break after updating, and how they recover; empty if nobody' },
        },
        required: ['slug', 'owner', 'branch', 'task', 'verify', 'userVisible', 'runtime', 'breaks'],
      },
    },
    followUps: { type: 'array', items: { type: 'string' } },
  },
  required: ['batch', 'title', 'items'],
}
const BUILD = {
  type: 'object',
  properties: {
    branch: { type: 'string' },
    commits: { type: 'array', items: { type: 'string' }, description: 'hash + subject' },
    verified: { type: 'array', items: { type: 'string' }, description: 'each check run and its result' },
    notVerified: { type: 'array', items: { type: 'string' } },
    notes: { type: 'string' },
  },
  required: ['branch', 'commits', 'verified'],
}
const FINDINGS = {
  type: 'object',
  properties: {
    findings: {
      type: 'array',
      items: {
        type: 'object',
        properties: {
          title: { type: 'string' },
          severity: { type: 'string', enum: ['high', 'medium', 'low'] },
          where: { type: 'string', description: 'file:line on the branch' },
          scenario: { type: 'string', description: 'concrete input/state and what goes wrong' },
          fix: { type: 'string' },
        },
        required: ['title', 'severity', 'where', 'scenario'],
      },
    },
  },
  required: ['findings'],
}
const VERDICT = {
  type: 'object',
  properties: { real: { type: 'boolean' }, why: { type: 'string' } },
  required: ['real', 'why'],
}

const houseRules =
  'House rules: everything in the repo is English (comments, commits like "[fix] Short summary", UI text); only docs/ko is Korean and changes together with docs/. ' +
  'Shortcuts use ⌘⌥⌃⇧ glyphs. Stage files explicitly; no screenshots or scratch files in commits; no Co-Authored-By. ' +
  'Run every git command in your own worktree (your starting directory). Never cd into, or run git -C on, the main checkout ' + REPO + ': it stays detached so branches are free. ' +
  'Real servers only in Docker containers named qurry-demo-<x> on free localhost ports (check with lsof; never 127.0.0.1:15432/:25432; never touch other containers; stop yours afterwards). ' +
  "Never push, merge, or comment on GitHub. Don't automate the user's running Qurry.app. " +
  'Disk is limited: never point CARGO_TARGET_DIR anywhere else (no per-agent or scratchpad target dirs) and delete any large scratch output you create. ' +
  'Machine limits (24 GB laptop, other agents run at the same time): before any cargo command run `export CARGO_TARGET_DIR=' + (A.targetDir || REPO + '/target') + ' CARGO_BUILD_JOBS=6` (one shared build dir; cargo waits for its lock — that is expected); ' +
  (A.sharedDbs ? 'use the shared test servers, do not start your own: ' + A.sharedDbs + ' — use your own schema/database/key prefix named after your item and drop it when done; ' : '') +
  'start a container only for a server version that is not shared (one at a time, named qurry-demo-<item>, stop and remove it as soon as you are done); run Playwright with one worker and one browser project at a time unless the task needs both. ' +
  'Principles: YAGNI (only what the task needs now, no speculative options or abstractions), KISS (simplest correct design), DRY (one source of truth; but no forced abstraction for things that only look alike).'

// ───────── Plan ─────────
phase('Plan')
// A plan made earlier (e.g. one phase of a larger plan) can be passed in: args.items (+ args.title); then the lead doesn't plan again
const plan = A.items?.length
  ? { batch: BATCH, title: A.title || '', items: A.items, followUps: A.followUps || [] }
  : await agent(
  `Plan this batch of Qurry work. Base branch: ${BASE}.${BATCH ? ` Batch branch: ${BATCH}.` : ' Choose the batch branch name.'}\n\n` +
    `Request:\n${A.request}\n\n` +
    'Read the relevant code first so the items are accurate (file names, functions). Split into independently mergeable items following your planning rules; ' +
    'put work that must edit the same function into one item. Return the plan in the given structure.',
  { agentType: 'qurry-lead', schema: PLAN, label: 'lead: plan' },
)
if (!plan || !plan.items?.length) return { error: 'The lead returned no plan' }
const batch = BATCH || plan.batch
log(`Plan: ${plan.items.length} item(s) → ${batch}: ${plan.items.map((i) => `${i.slug} (${i.owner})`).join(', ')}`)

// ───────── per item: Build → Review → Fix (no barrier between items) ─────────
// Commits must end up on the branch: if `git checkout <branch>` failed (checked out elsewhere), an agent can commit on a detached HEAD and the next review reads the old branch
const keepBranch = (branch) =>
  `Before you finish, make sure ${branch} points at your last commit: if \`git rev-parse ${branch}\` differs from \`git rev-parse HEAD\` and \`git merge-base --is-ancestor ${branch} HEAD\` succeeds, run \`git branch -f ${branch} HEAD\`; if the branch is not an ancestor, stop and say so in the report. Then run \`git checkout --detach\` so the branch is free for other worktrees.`
// Reviewers and skeptics read a branch in their own worktree, detached, so the branch stays free for its fixer
const readBranch = (branch) => `Start: \`git fetch origin -q; git checkout --detach ${branch}\` in your own worktree (your starting directory); never check out a branch by name. `
const lensesFor = (it) => [
  { key: 'security', type: 'qurry-security', focus: 'security and data safety (credentials, what reaches a database or an AI tool, injection, temp files), and who this breaks after an update' },
  ...(it.userVisible ? [{ key: 'ui', type: 'qurry-ui', focus: 'UI/UX and GUI–TUI parity, Storybook updated, themes, keyboard flow, error/empty/loading states' }] : []),
  ...(it.runtime ? [{ key: 'perf', type: 'qurry-perf', focus: 'performance and resources (blocking in async, leaks, unbounded growth, per-frame cost, large data)' }] : []),
  { key: 'correctness', type: 'general-purpose', focus: 'correctness: does it do what the task asked, edge cases, tests that actually prove it, regressions in nearby behavior; and design — YAGNI (speculative options/abstractions), KISS (needless complexity), DRY (duplicated logic or rules that should have one home)' },
]

const review = async (it, round) => {
  const lenses = lensesFor(it)
  const found = (
    await parallel(
      lenses.map((l) => () =>
        agent(
          readBranch(it.branch) +
            `Review branch ${it.branch} (diff: \`git diff ${BASE === 'main' ? 'origin/main' : BASE}...${it.branch}\`) for Qurry work item "${it.slug}".\n` +
            `Task it implements:\n${it.task}\n\nYour lens: ${l.focus}.\n` +
            'Read the code; run tests or real-server checks when a claim depends on behavior. Report only concrete problems with file:line and a scenario — no style nits. Empty list if it is fine. ' +
            'Do not modify any branch.',
          { agentType: l.type, isolation: 'worktree', schema: FINDINGS, phase: 'Review', label: `review r${round} ${l.key}: ${it.slug}` },
        ).then((r) => (r?.findings || []).map((f) => ({ ...f, lens: l.key }))),
      ),
    )
  )
    .filter(Boolean)
    .flat()
  // only medium/high findings get a skeptic; lows go unverified to the fixer (optional) and the PR follow-ups
  const lows = found.filter((f) => f.severity === 'low')
  const serious = found.filter((f) => f.severity !== 'low')
  if (!serious.length) return { confirmed: [], lows }
  const judged = await parallel(
    serious.map((f) => () =>
      agent(
        readBranch(it.branch) +
          `Try to REFUTE this review finding on branch ${it.branch} of Qurry. Check the code (and run it if needed). Default to real=false if you cannot confirm it.\n\n` +
          `${f.severity.toUpperCase()} [${f.lens}] ${f.title}\nWhere: ${f.where}\nScenario: ${f.scenario}`,
        { isolation: 'worktree', schema: VERDICT, phase: 'Review', label: `verify: ${f.title.slice(0, 40)}`, effort: 'high' },
      ).then((v) => ({ f, real: !!v?.real, why: v?.why || '' })),
    ),
  )
  const confirmed = judged.filter((j) => j.real).map((j) => j.f)
  const dropped = judged.length - confirmed.length
  if (dropped) log(`${it.slug} r${round}: ${dropped} finding(s) refuted by the skeptic, ${confirmed.length} confirmed`)
  return { confirmed, lows }
}

// Bounded pool: at most PARALLEL items in flight (the workflow's own cap is ~CPUs-2, too many builds for a laptop)
const pool = async (items, fn) => {
  const out = new Array(items.length)
  let next = 0
  const worker = async () => {
    while (next < items.length) {
      const k = next++
      out[k] = await fn(items[k], k).catch(() => null)
    }
  }
  await Promise.all(Array.from({ length: Math.min(PARALLEL, items.length) }, worker))
  return out
}
const build = (it) =>
    agent(
      `You own Qurry work item "${it.slug}" in batch ${batch}.\n` +
        `Start: \`git fetch origin -q\`. If branch ${it.branch} already exists, \`git checkout ${it.branch}\` and CONTINUE from its commits (an earlier run was interrupted; a "[wip]" commit holds unfinished work — review it, keep what is right, finish the task, and fold the wip into proper commits with \`git reset --soft\` to the base if that's cleaner). Otherwise \`git checkout -b ${it.branch} ${BASE === 'main' ? 'origin/main' : BASE}\` (a local branch is fine).\n\n` +
        `Task:\n${it.task}\n\nExpected files: ${(it.files || []).join(', ') || '(your call)'}\n` +
        `Verification you must run and report:\n- ${it.verify.join('\n- ')}\n` +
        (it.breaks ? `\nWho this could break after an update: ${it.breaks} — make the failure message tell them how to recover.\n` : '') +
        `\n${houseRules}\nCommit on ${it.branch} in focused commits. ${keepBranch(it.branch)} Return the build report.`,
      { agentType: OWNER_TYPE[it.owner] || 'general-purpose', isolation: 'worktree', schema: BUILD, phase: 'Build', label: `build: ${it.slug}` },
    )
const reviewAndFix = async (built, it) => {
    if (!built) return { item: it, built: null, open: ['build failed'] }
    let { confirmed: open, lows } = await review(it, 1)
    let round = 1
    const fixes = []
    while ((open.length || lows.length) && round <= MAX_FIX_ROUNDS) {
      const list = (fs) => fs.map((f, i) => `${i + 1}. ${f.severity} [${f.lens}] ${f.title} — ${f.where}\n   ${f.scenario}${f.fix ? `\n   Suggested: ${f.fix}` : ''}`).join('\n')
      const fixed = await agent(
        `Fix review findings on branch ${it.branch} (Qurry work item "${it.slug}").\n` +
          `Start: \`git fetch origin -q; git checkout ${it.branch}\`.\n\n` +
          (open.length ? `Confirmed (must fix):\n${list(open)}\n\n` : '') +
          (lows.length ? `Low severity, unverified (fix only if real and the fix is small; otherwise leave it and say why in notes):\n${list(lows)}\n\n` : '') +
          `Add a test for each fix where possible and re-run the item's checks:\n- ${it.verify.join('\n- ')}\n\n${houseRules}\n` +
          `Commit on the same branch. ${keepBranch(it.branch)} Return the build report.`,
        { agentType: OWNER_TYPE[it.owner] || 'general-purpose', isolation: 'worktree', schema: BUILD, phase: 'Fix', label: `fix r${round}: ${it.slug}` },
      )
      fixes.push(fixed)
      round++
      ;({ confirmed: open, lows } = await review(it, round))
    }
    if (open.length) log(`${it.slug}: ${open.length} finding(s) still open after ${MAX_FIX_ROUNDS} fix round(s)`)
    open = [...open, ...lows]
    return { item: it, built, fixes: fixes.filter(Boolean), open }
}
const results = await pool(plan.items, async (it) => reviewAndFix(await build(it), it))

const done = results.filter(Boolean)
const ready = done.filter((r) => r.built && !r.open.some((f) => typeof f === 'object' && f.severity === 'high'))
const held = done.filter((r) => !ready.includes(r))
if (held.length) log(`Held back (build failed or high-severity findings open): ${held.map((r) => r.item.slug).join(', ')}`)

// ───────── Integrate ─────────
phase('Integrate')
const integration = ready.length
  ? await agent(
      `Integrate the finished Qurry work items into batch branch ${batch} (base ${BASE}).\n` +
        `Start: \`git fetch origin -q\`; if ${batch} exists (locally or on origin) check it out, else \`git checkout -b ${batch} ${BASE === 'main' ? 'origin/main' : BASE}\`.\n` +
        `Merge these branches in order with merge commits:\n${ready.map((r) => `- ${r.item.branch} (${r.item.slug})`).join('\n')}\n\n` +
        `Item reports:\n${JSON.stringify(ready.map((r) => ({ slug: r.item.slug, task: r.item.task, verify: r.item.verify, breaks: r.item.breaks, build: r.built, fixes: r.fixes, open: r.open })), null, 1)}\n\n` +
        (held.length ? `Held back (do NOT merge; list them in the PR as follow-ups): ${held.map((r) => r.item.slug).join(', ')}\n` : '') +
        (plan.followUps?.length ? `Planned follow-ups: ${plan.followUps.join('; ')}\n` : '') +
        `Then run the full verification from your integration rules on the batch branch.\n` +
        (OPEN_PR
          ? `If everything passes: push ${batch} to origin (\`git push -u origin ${batch}\`) and open a PR to main titled "${plan.title}" (or update the existing open PR for ${batch}: append sections, keep what's there). The user merges; never merge.\n`
          : 'Do not push or open a PR; report what would go in it.\n') +
        `${houseRules.replace("Never push, merge, or comment on GitHub. ", 'Never merge. ')}\nWhen done, run \`git checkout --detach\` so ${batch} is free for the next run. Return: merged branches, verification commands and results, PR URL (if any), anything skipped.`,
      { agentType: 'qurry-lead', isolation: 'worktree', phase: 'Integrate', label: 'lead: integrate' },
    )
  : 'Nothing ready to integrate.'

return {
  batch,
  title: plan.title,
  items: done.map((r) => ({ slug: r.item.slug, owner: r.item.owner, branch: r.item.branch, built: !!r.built, open: r.open, breaks: r.item.breaks })),
  held: held.map((r) => r.item.slug),
  followUps: plan.followUps || [],
  integration,
}
