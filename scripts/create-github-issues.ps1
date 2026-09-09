param(
  [string]$Repository = "",
  [string]$BacklogPath = "docs/github-issues.json"
)

$ErrorActionPreference = "Stop"

if (-not (Get-Command gh -ErrorAction SilentlyContinue)) {
  throw "GitHub CLI is required."
}

if (-not (Test-Path $BacklogPath)) {
  throw "Backlog file not found: $BacklogPath"
}

if ([string]::IsNullOrWhiteSpace($Repository)) {
  $Repository = gh repo view --json nameWithOwner --jq ".nameWithOwner"
}

if ([string]::IsNullOrWhiteSpace($Repository)) {
  throw "Repository was not provided and could not be inferred from the current git remote."
}

$data = Get-Content $BacklogPath -Raw | ConvertFrom-Json

foreach ($label in $data.labels) {
  gh label create $label.name --repo $Repository --color $label.color --description $label.description --force
}

foreach ($milestone in $data.milestones) {
  $existing = gh api "repos/$Repository/milestones" --jq ".[] | select(.title == `"$($milestone.title)`") | .number"
  if ([string]::IsNullOrWhiteSpace($existing)) {
    gh api "repos/$Repository/milestones" -f title="$($milestone.title)" -f description="$($milestone.description)" | Out-Null
  }
}

foreach ($issue in $data.issues) {
  $existingIssue = gh issue list `
    --repo $Repository `
    --state all `
    --json title `
    --jq ".[] | select(.title == `"$($issue.title)`") | .title"

  if (-not [string]::IsNullOrWhiteSpace($existingIssue)) {
    Write-Host "[skip] $($issue.title)"
    continue
  }

  $labelArgs = @()
  foreach ($label in $issue.labels) {
    $labelArgs += @("--label", $label)
  }

  gh issue create `
    --repo $Repository `
    --title $issue.title `
    --body $issue.body `
    --milestone $issue.milestone `
    @labelArgs
}
