#!/usr/bin/env python3
"""
Smart scaffold generator for handoff documents.

Creates a new handoff document with auto-detected metadata:
- Current timestamp
- Project path
- Git branch (if available)
- Recent git log
- Modified/staged files
- Handoff chain linking

Usage:
    python create_handoff.py [task-slug] [--continues-from <previous-handoff>]
    python create_handoff.py "implementing-auth"
    python create_handoff.py "auth-part-2" --continues-from 2024-01-15-auth.md
    python create_handoff.py  # auto-generates slug from timestamp
"""

import argparse
import os
import re
import subprocess
import sys
from datetime import datetime
from pathlib import Path


def run_cmd(cmd: list[str], cwd: str = None) -> tuple[bool, str]:
    """Run a command and return (success, output)."""
    try:
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            cwd=cwd,
            timeout=10
        )
        return result.returncode == 0, result.stdout.strip()
    except (subprocess.TimeoutExpired, FileNotFoundError):
        return False, ""


def get_git_info(project_path: str) -> dict:
    """Gather git information from the project."""
    info = {
        "is_git_repo": False,
        "branch": None,
        "recent_commits": [],
        "modified_files": [],
        "staged_files": [],
    }

    # Check if git repo
    success, _ = run_cmd(["git", "rev-parse", "--git-dir"], cwd=project_path)
    if not success:
        return info

    info["is_git_repo"] = True

    # Get current branch
    success, branch = run_cmd(["git", "branch", "--show-current"], cwd=project_path)
    if success and branch:
        info["branch"] = branch

    # Get recent commits (last 5)
    success, log = run_cmd(
        ["git", "log", "--oneline", "-5", "--no-decorate"],
        cwd=project_path
    )
    if success and log:
        info["recent_commits"] = log.split("\n")

    # Get modified files (unstaged)
    success, modified = run_cmd(
        ["git", "diff", "--name-only"],
        cwd=project_path
    )
    if success and modified:
        info["modified_files"] = modified.split("\n")

    # Get staged files
    success, staged = run_cmd(
        ["git", "diff", "--name-only", "--cached"],
        cwd=project_path
    )
    if success and staged:
        info["staged_files"] = staged.split("\n")

    return info


def find_previous_handoffs(project_path: str) -> list[dict]:
    """Find existing handoffs in the project."""
    # CHANGED: Use docs/handoffs/ instead of docs/handoffs/
    handoffs_dir = Path(project_path) / "docs" / "handoffs"
    if not handoffs_dir.exists():
        return []

    handoffs = []
    for filepath in handoffs_dir.glob("*.md"):
        # Skip archive directory
        if "archive" in str(filepath):
            continue
        # Extract date from filename (format: YYYY-MM-DD-HHMMSS-slug.md)
        match = re.match(r"(\d{4}-\d{2}-\d{2}-\d{6})-(.+)\.md", filepath.name)
        if match:
            handoffs.append({
                "filename": filepath.name,
                "date": match.group(1),
                "slug": match.group(2),
                "filepath": str(filepath),
            })

    # Sort by date (newest first)
    handoffs.sort(key=lambda x: x["date"], reverse=True)
    return handoffs


def generate_slug() -> str:
    """Generate a default slug from timestamp."""
    return datetime.now().strftime("%Y-%m-%d-%H%M%S") + "-handoff"


def create_handoff(project_path: str, slug: str, continues_from: str = None) -> str:
    """Create a new handoff document."""
    # CHANGED: Use docs/handoffs/ instead of docs/handoffs/
    handoffs_dir = Path(project_path) / "docs" / "handoffs"
    handoffs_dir.mkdir(parents=True, exist_ok=True)

    # Gather git info
    git_info = get_git_info(project_path)

    # Generate filename
    timestamp = datetime.now().strftime("%Y-%m-%d-%H%M%S")
    filename = f"{timestamp}-{slug}.md"
    filepath = handoffs_dir / filename

    # Find previous handoffs for chain linking
    previous_handoffs = find_previous_handoffs(project_path)

    # Build the handoff content
    content = f"""# Handoff: {slug.replace('-', ' ').title()}

## Session Metadata
- Created: {datetime.now().strftime("%Y-%m-%d %H:%M:%S")}
- Project: {project_path}
"""

    if git_info["is_git_repo"]:
        content += f"- Branch: {git_info['branch'] or 'unknown'}\n"

    # Add recent commits
    if git_info["recent_commits"]:
        content += "\n### Recent Commits (for context)\n"
        for commit in git_info["recent_commits"]:
            content += f"  - {commit}\n"

    # Add handoff chain section
    content += "\n## Handoff Chain\n\n"

    if continues_from:
        content += f"- **Continues from**: [{continues_from}](./{continues_from})\n"
        # Try to extract title from previous handoff
        prev_path = handoffs_dir / continues_from
        if prev_path.exists():
            with open(prev_path, 'r') as f:
                first_line = f.readline().strip()
                if first_line.startswith("# "):
                    prev_title = first_line[2:]
                    content += f"  - Previous title: {prev_title}\n"
    elif previous_handoffs:
        most_recent = previous_handoffs[0]
        content += f"- **Continues from**: [{most_recent['filename']}](./{most_recent['filename']})\n"
        content += f"  - Previous title: {most_recent['slug'].replace('-', ' ').title()}\n"
    else:
        content += "- **Continues from**: None (first handoff in this project)\n"

    content += """- **Supersedes**: [list any older handoffs this replaces, or "None"]

> Review the previous handoff for full context before filling this one.

## Current State Summary

[TODO: Write one paragraph describing what was being worked on, current status, and where things left off]

## Codebase Understanding

### Architecture Overview

[TODO: Document key architectural insights discovered during this session]

### Critical Files

| File | Purpose | Relevance |
|------|---------|-----------|
| [TODO: Add critical files] | | |

### Key Patterns Discovered

[TODO: Document important patterns, conventions, or idioms found in this codebase]

## Work Completed

### Tasks Finished

- [ ] [TODO: List completed tasks]

### Files Modified

| File | Changes | Rationale |
|------|---------|-----------|
"""

    # Add modified files
    if git_info["modified_files"]:
        for f in git_info["modified_files"]:
            content += f"| {f} | Modified | |\n"
    if git_info["staged_files"]:
        for f in git_info["staged_files"]:
            content += f"| {f} | Staged | |\n"

    if not git_info["modified_files"] and not git_info["staged_files"]:
        content += "| [no modified files detected] | | |\n"

    content += """
### Decisions Made

| Decision | Options Considered | Rationale |
|----------|-------------------|-----------|
| [TODO: Document key decisions] | | |

## Pending Work

### Immediate Next Steps

1. [TODO: Most critical next action]
2. [TODO: Second priority]
3. [TODO: Third priority]

### Blockers/Open Questions

- [ ] [TODO: List any blockers or open questions]

### Deferred Items

- [TODO: Items deferred and why]

## Context for Resuming Agent

### Important Context

[TODO: This is the MOST IMPORTANT section - write critical information the next agent MUST know]

### Assumptions Made

- [TODO: List assumptions made during this session]

### Potential Gotchas

- [TODO: Document things that might trip up a new agent]

## Environment State

### Tools/Services Used

- [TODO: List relevant tools and their configuration]

### Active Processes

- [TODO: Note any running processes, servers, etc.]

### Environment Variables

- [TODO: List relevant env var NAMES only - NEVER include actual values/secrets]

## Related Resources

- [TODO: Add links to relevant docs and files]

---

**Security Reminder**: Before finalizing, run `python docs/handoffs/scripts/validate_handoff.py {filename}` to check for accidental secret exposure.
"""

    # Write the file
    with open(filepath, 'w') as f:
        f.write(content)

    return str(filepath)


def main():
    parser = argparse.ArgumentParser(
        description="Create a new handoff document with auto-detected metadata"
    )
    parser.add_argument(
        "slug",
        nargs="?",
        help="Task slug (e.g., 'implementing-auth'). Auto-generated if not provided."
    )
    parser.add_argument(
        "--continues-from",
        metavar="FILENAME",
        help="Link to previous handoff file (e.g., '2024-01-15-auth.md')"
    )

    args = parser.parse_args()

    # Get project path (current working directory)
    project_path = os.getcwd()

    # Generate slug if not provided
    slug = args.slug or generate_slug()

    # Create the handoff
    filepath = create_handoff(project_path, slug, args.continues_from)

    print(f"Created handoff document: {filepath}")
    print(f"\nNext steps:")
    print(f"1. Open {filepath}")
    print(f"2. Replace [TODO: ...] placeholders with actual content")
    print(f"3. Focus especially on 'Important Context' and 'Immediate Next Steps'")
    print(f"4. Run: python docs/handoffs/scripts/validate_handoff.py {os.path.basename(filepath)}")
    print(f"   (Checks for completeness and accidental secrets)")


if __name__ == "__main__":
    main()
