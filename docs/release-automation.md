# Release automation runbook

This runbook describes the exact unattended release path for `wakeup` and the Homebrew tap.

## Workflow sequence

Semantic release checks out `wakeup` with the built-in read-only token and `fetch-depth: 0`.

Semantic release then sets up Node 22.23.0, runs `npm ci --ignore-scripts`, mints a repository-scoped App token, and runs `semantic-release` with that token as `GITHUB_TOKEN`.

Semantic release creates the release tag, the tag-triggered `Release` workflow builds and publishes the assets, the successful `Release` workflow triggers `workflow_run`, and the tap updater runs last.

## Identity and secrets

The GitHub App slug is `usrivastava92-bot`.

The numeric integration ID is `4286241`.

The production environment variable `APP_CLIENT_ID` stores the nonsecret Client ID.

The production environment secret `APP_PRIVATE_KEY` stores the private key.

## Parameter table

| Parameter | Value |
| --- | --- |
| App slug | `usrivastava92-bot` |
| Numeric integration ID | `4286241` |
| Client ID variable | `vars.APP_CLIENT_ID` |
| Private-key secret | `secrets.APP_PRIVATE_KEY` |
| App owner | `usrivastava92` |
| Source repo | `usrivastava92/wakeup` |
| Target repo | `usrivastava92/homebrew-tap` |
| Token permission | `contents: write` |

## Permissions and scope

The App installation is present on seven repositories.

Each job must request a token scoped to exactly one repository.

When the source and target repositories share an owner, the App token action can mint a repository-scoped token directly.

When the source and target repositories have different owners, the same App action still works as long as the owner and repository scope are set explicitly.

The semantic-release token is scoped to `usrivastava92/wakeup` with `contents: write`.

The tap-updater token is scoped to `usrivastava92/homebrew-tap` with `contents: write`.

The source checkout only needs the built-in read-only token.

## Environment setup

Production must remain unattended.

Do not add required reviewers or a wait timer to the production environment.

Keep the environment secret and variable names stable so both workflows can mint App tokens without human intervention.

## Branch protection and rulesets

Use one default-branch ruleset that requires pull requests, requires zero approvals, and grants `always` bypass only to integration ID `4286241`.

Zero approvals means PRs are required but not reviewed.

## Direct bypass versus PR flow

Direct App bypass is preferred for this repo because it preserves unattended releases and keeps the release identity narrow.

If direct push becomes unavailable, switch the release path to a PR-based update and keep the same token downscoping.

## Copyable workflow snippets

```yaml
- uses: actions/checkout@34e114876b0b11c390a56381ad16ebd13914f8d5
  with:
    fetch-depth: 0
    persist-credentials: false
```

```yaml
- uses: actions/create-github-app-token@bcd2ba49218906704ab6c1aa796996da409d3eb1
  with:
    client-id: ${{ vars.APP_CLIENT_ID }}
    private-key: ${{ secrets.APP_PRIVATE_KEY }}
    owner: usrivastava92
    repositories: wakeup
    permission-contents: write
```

## Key rotation

Rotate by creating a new App private key, updating `APP_PRIVATE_KEY`, validating the workflows, and then deleting the old key.

Do not change the App slug unless you are intentionally migrating identities.

## Safe token verification

Verify repository scoping by listing the token installation repositories and confirming only the intended repository appears.

Do not test scope by writing to another repository.

## Commit author and pusher

The tap commit explicitly uses `github-actions[bot]` as the commit author.

Semantic-release commit attribution comes from plugin configuration and defaults, and it does not determine pusher authorization.

## Troubleshooting

`GH013` usually means the token or branch policy does not match the target repository or branch.

Missing installation usually means the App is not installed on the target repository or the token was not downscoped correctly.

Missing asset usually means the release did not publish one of the expected binaries.

Expired token usually means the job held the token too long and should mint a fresh one before retrying.

## Fix-forward recovery

If a release publishes correctly but the tap update fails, rerun the failed tap workflow run after fixing the policy issue.

If a formula commit lands incorrectly, revert the formula commit and let the next successful release regenerate it.
