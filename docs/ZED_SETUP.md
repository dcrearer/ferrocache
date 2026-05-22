# Transitioning to Claude Code in Zed

## Pre-Flight Check ✅

Your Claude Code environment is **ready for Zed**:

- ✅ Global settings: `~/.claude/settings.json` (cargo auto-permissions)
- ✅ Project settings: `.claude/settings.json` (git, edit, write permissions)
- ✅ Project documentation: `CLAUDE.md` (auto-loaded context)
- ✅ Memory system: `~/.claude/projects/.../memory/` (saved state)
- ✅ Architecture docs: All documentation in place

## Transition Steps

### 1. Update Zed (if needed)
```bash
# Check your Zed version - you need latest
# Open Zed → About Zed
```

### 2. Open Project in Zed
```bash
cd ~/Dev/rust/ferrocache
zed .
```

### 3. Access Claude Code via ACP
1. Open **Agent Panel** (look for Plus menu or AI panel)
2. Find **Claude Code** in available agents
3. Select it to activate

### 4. Verify Settings are Loaded

**Test in Zed's Claude Code:**
```
"Check the Cargo.toml edition"
```

**Expected:** Should read the file without permission prompt (auto-allowed via settings)

### 5. Test Auto-Permissions

**Try a cargo command:**
```
"Run cargo check"
```

**Expected:** Should execute immediately without asking for permission

### 6. Verify Project Context

**Ask about the project:**
```
"What phase are we in for FerroCache development?"
```

**Expected:** Should know you're in Month 1 planning phase, 6-layer architecture, etc.

## Key Differences from CLI

### What's Better in Zed:

1. **Visual Code Review**
   - See changes with syntax highlighting
   - Accept/reject individual hunks
   - Multi-file changes visible side-by-side

2. **Integrated Workflow**
   - No terminal switching
   - LSP support (go-to-definition, etc.)
   - Task list in sidebar

3. **Real-time Feedback**
   - Watch Claude's edits appear live
   - Better for understanding refactors
   - Easier to spot issues

### What to Be Aware of:

1. **Plan Mode** - Not yet available in Zed (use CLI for this)
2. **Some Slash Commands** - Awaiting SDK support
3. **Beta Status** - Features still being added

## Recommended Workflow

### For Planning (Current Phase):
Use **Zed + Claude Code** for:
- Architecture discussions
- Design decisions
- Documentation updates
- Pseudocode/interface design

### When You Start Coding (Month 2):
Use **Zed + Claude Code** for:
- Implementation with visual feedback
- Multi-file refactoring
- Code review (accept/reject hunks)
- Real-time testing

### For Advanced Features (if needed):
Fall back to **CLI** for:
- Plan mode (when it becomes relevant)
- Complex automation with hooks
- Advanced slash commands not yet in SDK

## Testing Your Setup

### Test 1: Read Operation
```
"Read the CLAUDE.md file and summarize the project architecture"
```
✅ Should work without permission prompt

### Test 2: Cargo Command
```
"Run cargo check to verify the project compiles"
```
✅ Should execute immediately (auto-allowed)

### Test 3: Context Awareness
```
"What are the 6 layers in our architecture?"
```
✅ Should accurately describe Layers 1-6 from docs

### Test 4: Edit Operation
```
"Add a comment to src/main.rs explaining this is a distributed cache"
```
✅ Should edit without prompting (allowed path)

### Test 5: Documentation Update
```
"Update PROJECT_PLAN.md to check off the first task in Month 1"
```
✅ Should edit documentation files

## Troubleshooting

### Settings Not Loading?
- Check `.claude/settings.json` exists in project root
- Verify `~/.claude/settings.json` for global settings
- Restart Zed after changes

### Permissions Still Prompting?
- Settings might not be synced yet
- Try explicit permission: "Allow cargo commands"
- Check settings.json syntax is valid JSON

### Claude Doesn't Know Project Context?
- Verify `CLAUDE.md` exists in project root
- Check file isn't in .gitignore
- Try: "Read CLAUDE.md" to manually load

### Can't Find Claude Code in Agent Panel?
- Verify Zed is latest version
- Check you're logged in to Claude
- Look in Plus menu or AI assistant panel

## Quick Reference Commands

### Architecture & Planning:
```
"Compare DashMap vs sharded RwLock for our cache design"
"What concurrency patterns should we use for read-heavy workloads?"
"Design the CacheEntry struct interface"
```

### Documentation:
```
"Update ARCHITECTURE.md with our decision about [X]"
"Add this design decision to PROJECT_PLAN.md"
"Document the tradeoffs of [approach]"
```

### When Ready to Code (Month 2):
```
"Implement the Cache trait interface"
"Add unit tests for CacheEntry"
"Run cargo test and show results"
```

## Benefits for FerroCache Development

### Month 1 (Current - Planning):
- Better visualization of architecture docs
- Easy documentation updates
- Side-by-side comparison of design alternatives

### Month 2 (Implementation):
- See concurrent data structure changes clearly
- Visual review of Tokio async code
- Multi-file refactoring visibility

### Month 3 (Observability):
- Review metric instrumentation visually
- See tracing spans in context
- Clear view of deployment config changes

## Next Steps

1. ✅ Open Zed in ferrocache directory
2. ✅ Activate Claude Code via Agent Panel
3. ✅ Run the 5 test commands above
4. 🚀 Continue Month 1 planning with better tooling!

## Feedback

If you encounter issues or missing features:
- Report at: https://github.com/zed-industries/zed/issues
- Or: https://github.com/anthropics/claude-code/issues

Your setup is solid - everything should "just work" in Zed!
