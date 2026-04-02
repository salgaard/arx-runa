# SSOT Architecture Information Flow

```mermaid
graph TD
    subgraph "Canonical Source"
        DESIGNS["docs/architecture/designs/<design-name>/design.md<br/>(Design Documents)<br/>━━━━━━━━━━━━━━━━<br/>Complete technical specs<br/>Wire formats, schemas, parameters<br/>Security analysis, trade-offs"]
    end
    
    subgraph "AI Agent Layer"
        RULES[".claude/rules/*.md<br/>(AI Agent Rules)<br/>━━━━━━━━━━━━━━━━<br/>Brief constraint summaries<br/>References to design docs<br/>Path-specific guidance"]
        
        INSTRUCTIONS[".github/instructions/*.instructions.md<br/>(GitHub Copilot Rules)<br/>━━━━━━━━━━━━━━━━<br/>Synced from .claude/rules/<br/>Frontmatter transformed"]
    end
    
    subgraph "Reference-Based"
        ROADMAP["docs/roadmap.md<br/>(Implementation Logistics)<br/>━━━━━━━━━━━━━━━━<br/>Phase dependencies<br/>Test criteria<br/>ADR deliverables<br/>References to designs"]
        
        CLAUDE["CLAUDE.md<br/>(High-Level Charter)<br/>━━━━━━━━━━━━━━━━<br/>Core principles<br/>Tech stack names<br/>Hard constraints<br/>References to designs"]
    end
    
    DESIGNS -->|"Summarized in"| RULES
    RULES -->|"/copilot-sync<br/>frontmatter transform"| INSTRUCTIONS
    DESIGNS -.->|"References<br/>with logistics"| ROADMAP
    DESIGNS -.->|"Informs<br/>high-level"| CLAUDE
    
    classDef canonical fill:#4CAF50,stroke:#2E7D32,color:#fff
    classDef agent fill:#FF9800,stroke:#E65100,color:#fff
    classDef reference fill:#9C27B0,stroke:#6A1B9A,color:#fff
    
    class DESIGNS canonical
    class RULES,INSTRUCTIONS agent
    class ROADMAP,CLAUDE reference
```

## Legend

- **Canonical Source** (Green): Authoritative technical specifications. Edit here first.
- **AI Agent Layer** (Orange): Brief summaries that reference canonical sources.
- **Reference-Based** (Purple): High-level documents that reference designs, not duplicate them.

## Information Flow

1. **Design → Rules**: Rule files summarize constraints and reference design docs for details
2. **Rules → Sync**: Rules synced to GitHub Copilot via `/copilot-sync`
3. **Design → Reference**: Roadmap and CLAUDE.md reference designs for details

## Key Principle

**Design documents are the single source of truth**. AI agents read rule summaries for quick context, then consult design docs for authoritative details. When parameters change:

1. Update the design document
2. Update rule summary if needed
3. Run `/copilot-sync`
