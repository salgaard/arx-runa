# **Evolutionary Architectures in Rust: Engineering Multi-Agent Systems for Design Integrity and Technical Debt Mitigation**

The rapid maturation of the Rust programming language has transitioned the ecosystem from a focus on foundational memory safety to the pursuit of high-level architectural sustainability. In modern software engineering, particularly within the context of decentralized systems and complex application frameworks like Arx Runa, the challenge is no longer merely writing code that compiles, but ensuring that the resulting system architecture remains resilient to the accelerating pressures of feature expansion and technological shifts.1 The emergence of Large Language Model (LLM) agents as primary contributors to codebases introduces a novel dynamic where code can be generated at a velocity that far outpaces traditional human-centric review processes. This necessitates the development of an "architecture-reviewer" subagent—a specialized entity designed to operate at a higher level of abstraction than standard linters or basic reviewers, focusing on the preservation of structural boundaries, the enforcement of the Single Responsibility Principle (SRP), and the strategic management of technical debt.3

## **The Limitations of Static Canonical Designs**

Traditional software architecture often relies on canonical design documents—static artifacts that define the "ideal" state of a system. However, in a rapidly evolving codebase, these documents frequently become inhibitors to progress rather than enforcers of quality. Architectural "ossification" occurs when a system is so heavily layered with abstractions that every minor change requires a cascade of updates across multiple modules.5 When a developer or an AI agent follows these canonical designs blindly, they may inadvertently introduce "design debt"—a state where the code adheres to a rule that has become suboptimal for the current requirements of readability and maintainability.6  
The existing rust-reviewer model, while effective for identifying logic flaws and ensuring basic compliance with rules, operates within the constraints of these canonical designs. To achieve true architectural health, a secondary agent—the architecture-reviewer—must be empowered to look past these static contracts. This agent serves as a "merciless critic" that identifies when a design decision, no matter how well-intentioned, has begun to compromise the system's evolutionary capacity.4 This critical perspective is essential for preventing the accumulation of "Model-Stack Workaround Debt" and "Model Dependency Debt," where logic is contorted to fit the limitations of specific AI models or outdated architectural mandates.6

### **Comparative Analysis of Reviewer Scopes**

The distinction between a standard reviewer and an architectural reviewer is fundamental to maintaining a high-velocity development cycle. While the former focuses on the "what" and "how" of a change, the latter focuses on the "why" and its long-term implications.

| Feature | Standard Rust Reviewer | Architecture Reviewer |
| :---- | :---- | :---- |
| **Primary Goal** | Correctness, safety, and rule compliance. | Structural integrity, modularity, and evolutionary health. |
| **Authority** | Rigid adherence to .claude/rules/\*.md. | Critical evaluation of rules; identifies when rules need updating. |
| **Scope** | Uncommitted diffs and directly affected modules. | Crate boundaries, dependency graphs, and cross-module coupling. |
| **Focus** | unwrap(), logic flaws, race conditions, tests. | SRP violations, abstraction leakage, and design debt.4 |
| **Output** | Actionable HIGH/MEDIUM/LOW findings for immediate fix. | Strategic recommendations for refactoring and rule evolution. |

## **The Multi-Agent Orchestration Framework**

To effectively govern a Rust codebase, the architecture-reviewer must be coupled with a "problem-solver" agent in a coordinated multi-agent workflow. This orchestration is not merely a linear sequence of tasks but a sophisticated feedback loop that mirrors the interactions of a senior architect and a lead developer.10 The goal of this coupling is to create a self-correcting system where code quality and architectural rules evolve in tandem.12

### **Sequential and Hierarchical Orchestration Patterns**

The interaction between the reviewer and the solver can follow several proven architectural patterns for AI agents. A "Hierarchical" or "Hub-Spoke" pattern is often the most effective for complex refactoring tasks.13 In this model, a central orchestrator dispatches the current state of the codebase to the architecture-reviewer. The reviewer identifies structural weaknesses—such as a module that has grown to 1,500 lines or a file that handles multiple distinct actors—and generates a set of high-level refactoring recommendations.14  
The "problem-solver" agent then takes these recommendations as input. Critically, the solver must be permitted to go outside the canonical designs if it can demonstrate an optimal solution that improves maintainability. However, this freedom must be governed by a "Design Update Protocol." If the solver deviates from a rule, it is required to:

1. **Flag the Deviation**: Explicitly identify which rule or design section is being challenged.  
2. **Provide Rationale**: Explain why the current rule is suboptimal and how the new solution improves the system.  
3. **Propose Rule Updates**: Generate a draft for an updated .claude/rules/\*.md file that reflects the new architectural standard.12

This protocol ensures that technical debt is made visible and that the system's "tribal knowledge" is continuously codified into the project's living documentation.7

### **Verification Nodes and Approval Gates**

To prevent the AI from "vibe coding"—a state where output is accepted without rigorous verification—the workflow should include "verification nodes" at handoff boundaries.13 These nodes act as checkpoints where the architecture-reviewer or a "Compiler Agent" validates the output of the solver. In the Rust context, this involves running the full test suite, checking for linter warnings, and ensuring that no new unwrap() calls have been introduced in production paths.15  
"Approval Gates" are human-in-the-loop (HITL) moments where the proposed architectural changes and rule updates are presented to a human architect for final judgment.15 This is particularly important for high-stakes changes, such as those involving cryptography, authentication, or sensitive data storage.9

## **Core Architectural Principles for the Rust Subagent**

The architecture-reviewer must be anchored by a set of robust design principles that are particularly relevant to the Rust programming language. These principles go beyond basic linting to address the underlying reasons for code change and system evolution.20

### **The Single Responsibility Principle (SRP) and Actor Theory**

In Rust, the Single Responsibility Principle is often misinterpreted as "a function should do one thing." At the architectural level, SRP means that a module, struct, or crate should have only one reason to change, which is defined by its responsibility to a specific "actor".20 An actor is a person, group, or system that has the authority to make decisions about a module's features.  
For example, a Report struct that handles both the calculation of payroll data and the formatting of that data for email delivery violates SRP. The payroll logic is accountable to the Accounting department, while the formatting logic is accountable to the Communications team. If the Accounting department changes its calculation method, the formatting logic may be inadvertently broken if they are tightly coupled in the same module.20 The architecture-reviewer should look for these hidden entanglements and suggest splitting the code into distinct modules: PayrollCalculator and ReportFormatter.

### **Hexagonal Architecture and Port/Adapter Isolation**

Hexagonal Architecture (Ports and Adapters) is a natural fit for Rust due to the language's powerful trait system.9 The core principle is the isolation of the domain logic from the "external world"—databases, HTTP servers, and third-party APIs. The domain layer defines "ports" (traits), and the infrastructure layer provides "adapters" (implementations).23

| Component | Responsibility | Rust Implementation |
| :---- | :---- | :---- |
| **Domain** | Pure business logic and invariants. | Structs, Enums, and Traits (Ports).23 |
| **Use Cases** | Coordination logic and flow management. | Structs with methods calling domain logic.23 |
| **Ports** | Interface contracts for the domain. | pub trait definitions in the domain layer.9 |
| **Adapters** | Concrete implementations for ports. | impl Trait for MyStruct in the infrastructure layer.23 |

The architecture-reviewer should enforce the "Dependency Rule": dependencies must always flow inward toward the domain. The domain layer must never depend on an adapter. If the reviewer finds a domain struct that imports sqlx or tokio, it has detected a major architectural failure.9

### **The Typestate Pattern and State Safety**

Rust's type system allows for the encoding of state transitions at compile-time, a pattern known as "typestate".9 This is a critical principle for ensuring API safety and preventing runtime errors. Instead of using a single struct with an enum State field and checking that state at runtime, the reviewer should look for opportunities to use distinct types for each state.  
For example, a FileUploader should only have an upload() method when it is in the Authenticated state. By using types like FileUploader\<Unauthenticated\> and FileUploader\<Authenticated\>, the developer can ensure that upload() is literally uncallable until authentication is successful. This eliminates a vast class of potential logic bugs and "invalid state transitions".9

## **Identifying and Managing Architectural Debt**

Technical debt is an inevitable part of software development, but "Architectural Debt"—the result of poor coupling and ossified designs—is particularly damaging to long-term productivity.7 The architecture-reviewer must be equipped to identify the warning signs of this debt before it becomes unmanageable.

### **Heuristics for the Architecture Reviewer**

To perform a deep audit, the reviewer should look for the following "smells" that indicate a breakdown in design integrity:

1. **Large Files and "God Objects"**: Files with over 400-500 lines or structs that implement more than five distinct traits are often signs that SRP has been violated.14 These monolithic designs make testing and maintenance exponentially harder.  
2. **Lack of Crate Visibility Discipline**: Rust's pub(crate) and pub(super) visibility markers are essential for information hiding.9 If everything in a crate is marked pub, the crate has no clear internal boundary, and any internal change can break external consumers.  
3. **The "unwrap()" and "expect()" Addiction**: Overusing these methods indicates a failure to design robust error-handling paths.26 In production code, every error should be a first-class citizen of the API, typically via custom error enums and the Result\<T, E\> type.  
4. **Inconsistent Abstractions**: If the codebase uses multiple different patterns for the same concern (e.g., three different ways to handle database connections), it introduces "Design Debt" that confuses developers and agents alike.7  
5. **Model-Specific Workarounds**: In the LLM era, developers may write "Model-Stack Workaround Debt"—code that exists solely to fix a hallucination or a context-window limitation of a specific AI model.6 The reviewer should flag these as volatile implementation details that need abstraction.

### **Quantitative Metrics for System Coupling**

The reviewer can leverage system-wide metrics to quantify the degree of coupling and the risk of "evolutionary coupling"—where files are frequently changed together even when they aren't logically related.27

| Metric | Target Value | Architectural Implication |
| :---- | :---- | :---- |
| **Crate Dependency Depth** | \< 4 | Deep dependency chains increase the "ripple effect" of changes.7 |
| **Public API Surface Area** | Minimal | A smaller public API reduces the risk of breaking downstream consumers.9 |
| **Cyclomatic Complexity** | \< 15 per function | High complexity makes logic hard to reason about and test.18 |
| **Co-change Frequency** | Low | Frequent co-changes between unrelated files suggest an implicit dependency that should be made explicit.27 |

## **Engineering the.claude/subagents/architecture-reviewer.md**

The actual implementation of the subagent requires a clear definition of its role, authority, and output format. It should be designed as a "Merciless Critic" that specifically looks for design flaws and architectural debt.

### **Subagent Role Definition**

The subagent should be defined in a .md file that Claude can ingest. Its system prompt should explicitly instruct it to ignore style nits and focus on high-level design.  
**Role**: Senior Rust Architect and Merciless Critic.  
**Authority**: Can challenge .claude/rules/\*.md and canonical designs if they impede maintainability.  
**Scope**: All Rust files, focusing on crate boundaries, trait definitions, and module hierarchies.

### **Detailed Review Phases for the Subagent**

The review should be conducted in structured phases, each with a specific architectural focus:

1. **Phase 1: Boundary Integrity**  
   * Enforce the "One Concern Per File" rule.  
   * Check for "One Reason to Change" (SRP) at the module and struct level.  
   * Audit visibility (pub vs pub(crate)) and module re-exports.  
2. **Phase 2: Abstraction Quality**  
   * Identify "Type Laundering" and suggest Newtypes for domain concepts.  
   * Evaluate trait usage: are they providing genuine polymorphism or just adding indirection debt?  
   * Check for "Typestate" opportunities in complex workflows.  
3. **Phase 3: Dependency Flow**  
   * Enforce the Hexagonal/DDD "Inward Dependency" rule.  
   * Flag any leakage of infrastructure details into the domain layer.  
   * Identify circular dependencies and suggest "Dependency Inversion" patterns.  
4. **Phase 4: Technical Debt Audit**  
   * Identify "SATD" (Self-Admitted Technical Debt) in comments and quantify its impact.6  
   * Look for "Model-Stack Workarounds" and "Model Dependency Debt."  
   * Flag "ossified" code that adheres to outdated designs at the expense of maintainability.  
5. **Phase 5: Rule Evolution Recommendations**  
   * If a rule is found to be suboptimal, propose a specific update to .claude/rules/.  
   * Group findings by "Structural Risk" rather than just "Severity."

### **Output Format for Architectural Findings**

The architecture-reviewer should use a format that highlights the structural impact of its findings:  
STRUCTURAL\_RISK —  
Category: \<SRP Violation / Abstraction Leak / Design Debt\>  
Files:  
Design Conflict:  
Why it Matters:  
Recommendation:

## **Integrating "Design Friction" into the Workflow**

One of the most significant risks in AI-assisted development is the lack of "reflective thinking." AI agents tend to follow the "path of least resistance," which often leads to cutting architectural corners.28 To counteract this, the architecture-reviewer should be programmed to introduce "Design Friction"—intentional moments of negative user experience or delayed feedback that force the developer or the solver agent to pay attention to critical design decisions.29

### **Mechanisms for Security-Enhancing and Design-Enhancing Friction**

Friction can be introduced through several mechanisms within the multi-agent system:

* **Polymorphic Dialogues**: The reviewer can require the solver to explain its reasoning in multiple ways or to provide two alternative refactoring plans before proceeding.29  
* **Disorienting Dilemmas**: When the reviewer detects a complex architectural trade-off, it can stop the workflow and present a "dilemma" to the human architect, requiring a manual decision before the AI continues.28  
* **Audit Logs for Rule Deviations**: Every time the solver goes outside a canonical design, it must be logged in a "Technical Debt Ledger" that is reviewed by the human team during weekly sprints.7

By designing-in friction, the system ensures that architectural choices are made with agency and knowledge, rather than through habitual compliance with outdated rules.28

## **Managing Technical Debt Evolution in the AI Era**

The nature of technical debt is changing as AI becomes a primary producer of code. Research indicates that LLM-based systems have unique debt patterns, particularly in "Deployment and Monitoring" and "Pretraining" stages.6 For a Rust application like Arx Runa, this means that the architecture-reviewer must be vigilant about "Model Dependency Debt," where the codebase becomes so tightly coupled to a specific model's prompting style or output quirks that switching models becomes prohibitively expensive.6

### **Strategies for Long-Term Maintenance**

To ensure the long-term health of the codebase, the multi-agent system must adopt a "Continuous Refactoring" mindset.12

1. **Treat Rules as First-Class Code**: Architectural rules in .claude/rules/ should be version-controlled, tested, and updated as frequently as the code itself.  
2. **Quantify Maintenance Time**: Use tools to track how much time is spent on maintenance versus new feature development. If maintenance costs spike, it is a signal for a deep architectural audit.7  
3. **Regular Audit Schedule**: Even if no new features are being added, the architecture-reviewer should periodically scan the entire codebase for emerging smells and ossified patterns.9  
4. **Isolate AI-Specific Logic**: Any code that is written to support AI agents (e.g., specific metadata or context-generation logic) should be isolated in its own crate or module to prevent it from polluting the core business domain.

## **Conclusion: The Future of Agentic Architectural Governance**

The pursuit of clean code in Rust is no longer a solo human endeavor but a collaborative process between human craftsmanship and AI orchestration.30 By engineering a specialized architecture-reviewer subagent that operates beyond the constraints of static designs, organizations can build systems that are not only safe and correct but also fundamentally maintainable.1  
The coupling of a "Merciless Critic" reviewer with a "Problem-Solver" agent creates a dynamic environment where the codebase and the architectural rules evolve together. This "Living Spec" approach ensures that design debt is minimized and that the system remains resilient to the pressures of the future.12 By focusing on the Single Responsibility Principle, Hexagonal Isolation, and the Typestate pattern, and by intentionally introducing Design Friction, developers can leverage the power of AI while maintaining the high standards of structural integrity required for production-grade Rust systems.  
Ultimately, the goal of this multi-agent architecture is to move from "vibe coding" to a disciplined, "agentic engineering" paradigm where every line of code and every architectural rule is a conscious, verifiable decision.30 In this new landscape, the architect's role shifts from a manual code reviewer to an orchestrator of intelligent agents, ensuring that the system's "One Reason to Change" remains a clear and manageable principle across the entire evolutionary lifecycle.

#### **Works cited**

1. The State of Rust Ecosystem 2025 | The RustRover Blog, accessed April 15, 2026, [https://blog.jetbrains.com/rust/2026/02/11/state-of-rust-2025/](https://blog.jetbrains.com/rust/2026/02/11/state-of-rust-2025/)  
2. Rust's Strategic Advantage | sysid blog, accessed April 15, 2026, [https://sysid.github.io/rusts-strategic-advantage/](https://sysid.github.io/rusts-strategic-advantage/)  
3. Subagents in Visual Studio Code, accessed April 15, 2026, [https://code.visualstudio.com/docs/copilot/agents/subagents](https://code.visualstudio.com/docs/copilot/agents/subagents)  
4. Agents are life : r/ClaudeAI \- Reddit, accessed April 15, 2026, [https://www.reddit.com/r/ClaudeAI/comments/1oux9ab/agents\_are\_life/](https://www.reddit.com/r/ClaudeAI/comments/1oux9ab/agents_are_life/)  
5. Master Hexagonal Architecture in Rust \- Hacker News, accessed April 15, 2026, [https://news.ycombinator.com/item?id=41518698](https://news.ycombinator.com/item?id=41518698)  
6. Self-Admitted Technical Debt in LLM Software: An Empirical Comparison with ML and Non-ML Software \- arXiv, accessed April 15, 2026, [https://arxiv.org/html/2601.06266v1](https://arxiv.org/html/2601.06266v1)  
7. Technical debt: a strategic guide for 2026 \- Monday.com, accessed April 15, 2026, [https://monday.com/blog/rnd/technical-debt/](https://monday.com/blog/rnd/technical-debt/)  
8. Self-Admitted Technical Debt in LLM Software: An Empirical Comparison with ML and Non-ML Software \- arXiv, accessed April 15, 2026, [https://arxiv.org/html/2601.06266v3](https://arxiv.org/html/2601.06266v3)  
9. rust-architecture-patterns \- Skill | Smithery, accessed April 15, 2026, [https://smithery.ai/skills/davincible/rust-architecture-patterns](https://smithery.ai/skills/davincible/rust-architecture-patterns)  
10. AI Agent Orchestration Patterns \- Azure Architecture Center | Microsoft Learn, accessed April 15, 2026, [https://learn.microsoft.com/en-us/azure/architecture/ai-ml/guide/ai-agent-design-patterns](https://learn.microsoft.com/en-us/azure/architecture/ai-ml/guide/ai-agent-design-patterns)  
11. Fragments: February 13 \- Martin Fowler, accessed April 15, 2026, [https://martinfowler.com/fragments/2026-02-13.html](https://martinfowler.com/fragments/2026-02-13.html)  
12. Continuous Refactoring with LLMs: Patterns That Work in Production \- DEV Community, accessed April 15, 2026, [https://dev.to/dextralabs/continuous-refactoring-with-llms-patterns-that-work-in-production-136e](https://dev.to/dextralabs/continuous-refactoring-with-llms-patterns-that-work-in-production-136e)  
13. Multi-Agent AI Architecture: Patterns for Enterprise Development | Augment Code, accessed April 15, 2026, [https://www.augmentcode.com/guides/multi-agent-ai-architecture-patterns-enterprise](https://www.augmentcode.com/guides/multi-agent-ai-architecture-patterns-enterprise)  
14. Rust DDD Kickstart: My Journey Building a Domain-Driven Backend ..., accessed April 15, 2026, [https://medium.com/@nic.dalmasso/rust-ddd-kickstart-my-journey-building-a-domain-driven-backend-template-f19869509ce3](https://medium.com/@nic.dalmasso/rust-ddd-kickstart-my-journey-building-a-domain-driven-backend-template-f19869509ce3)  
15. Single-responsibility agents and multi-agent workflows in AI-powered development tools, accessed April 15, 2026, [https://www.epam.com/insights/ai/blogs/single-responsibility-agents-and-multi-agent-workflows](https://www.epam.com/insights/ai/blogs/single-responsibility-agents-and-multi-agent-workflows)  
16. Rethinking the Software Development Lifecycle: Integrating AI into Every Stage \- GoCodeo, accessed April 15, 2026, [https://www.gocodeo.com/post/rethinking-the-software-development-lifecycle-integrating-ai-into-every-stage](https://www.gocodeo.com/post/rethinking-the-software-development-lifecycle-integrating-ai-into-every-stage)  
17. rahulvrane/awesome-claude-agents: collection of awesome claude code subagents\! \- GitHub, accessed April 15, 2026, [https://github.com/rahulvrane/awesome-claude-agents](https://github.com/rahulvrane/awesome-claude-agents)  
18. RefAgent: A Multi-agent LLM-based Framework for Automatic Software Refactoring \- arXiv, accessed April 15, 2026, [https://arxiv.org/html/2511.03153v2](https://arxiv.org/html/2511.03153v2)  
19. (PDF) Human-Centered Pathways to Trustworthy AI in Healthcare: A Comparative Analysis of Explainable AI, Human-in-the-Loop, Hybrid AI, and Uncertainty Quantification Techniques \- ResearchGate, accessed April 15, 2026, [https://www.researchgate.net/publication/401321377\_Human-Centered\_Pathways\_to\_Trustworthy\_AI\_in\_Healthcare\_A\_Comparative\_Analysis\_of\_Explainable\_AI\_Human-in-the-Loop\_Hybrid\_AI\_and\_Uncertainty\_Quantification\_Techniques](https://www.researchgate.net/publication/401321377_Human-Centered_Pathways_to_Trustworthy_AI_in_Healthcare_A_Comparative_Analysis_of_Explainable_AI_Human-in-the-Loop_Hybrid_AI_and_Uncertainty_Quantification_Techniques)  
20. SOLID Principles in Rust: A Practical Guide \- 00 | 40tude, accessed April 15, 2026, [https://www.40tude.fr/docs/06\_programmation/rust/022\_solid/solid\_00.html](https://www.40tude.fr/docs/06_programmation/rust/022_solid/solid_00.html)  
21. Embracing the Single Responsibility Principle in Rust | CodeSignal Learn, accessed April 15, 2026, [https://codesignal.com/learn/courses/clean-coding-with-structs-and-traits-in-rust/lessons/embracing-the-single-responsibility-principle-in-rust](https://codesignal.com/learn/courses/clean-coding-with-structs-and-traits-in-rust/lessons/embracing-the-single-responsibility-principle-in-rust)  
22. Master Hexagonal Architecture in Rust \- How To Code It, accessed April 15, 2026, [https://www.howtocodeit.com/guides/master-hexagonal-architecture-in-rust](https://www.howtocodeit.com/guides/master-hexagonal-architecture-in-rust)  
23. Hexagonal architecture in Rust. Tutorial index | by Luca Corsetti | Medium, accessed April 15, 2026, [https://medium.com/@lucorset/hexagonal-architecture-in-rust-72f8958eb26d](https://medium.com/@lucorset/hexagonal-architecture-in-rust-72f8958eb26d)  
24. Technical Debt and System Architecture: The Impact of Coupling on Defect-related Activity \- Article \- Faculty & Research, accessed April 15, 2026, [https://www.hbs.edu/faculty/Pages/item.aspx?num=51343](https://www.hbs.edu/faculty/Pages/item.aspx?num=51343)  
25. Design principles \- Rust Design Patterns, accessed April 15, 2026, [https://rust-unofficial.github.io/patterns/additional\_resources/design-principles.html](https://rust-unofficial.github.io/patterns/additional_resources/design-principles.html)  
26. The 7 Rust Anti-Patterns That Are Secretly Killing Your Performance (and How to Fix Them in 2025\!) | by Sreeved Vp | solo devs | Medium, accessed April 15, 2026, [https://medium.com/solo-devs/the-7-rust-anti-patterns-that-are-secretly-killing-your-performance-and-how-to-fix-them-in-2025-dcebfdef7b54](https://medium.com/solo-devs/the-7-rust-anti-patterns-that-are-secretly-killing-your-performance-and-how-to-fix-them-in-2025-dcebfdef7b54)  
27. Understanding Evolutionary Coupling by Fine-grained Co-change Relationship Analysis, accessed April 15, 2026, [https://www.cs.drexel.edu/\~yfcai/papers/2019/icpc2019.pdf](https://www.cs.drexel.edu/~yfcai/papers/2019/icpc2019.pdf)  
28. How designing for reflection could support privacy self-management \- First Monday, accessed April 15, 2026, [https://firstmonday.org/ojs/index.php/fm/article/view/9358/8051](https://firstmonday.org/ojs/index.php/fm/article/view/9358/8051)  
29. The Framework of Security-Enhancing Friction: How UX Can Help Users Behave More Securely \- ResearchGate, accessed April 15, 2026, [https://www.researchgate.net/publication/348854280\_The\_Framework\_of\_Security-Enhancing\_Friction\_How\_UX\_Can\_Help\_Users\_Behave\_More\_Securely](https://www.researchgate.net/publication/348854280_The_Framework_of_Security-Enhancing_Friction_How_UX_Can_Help_Users_Behave_More_Securely)  
30. 10 tips for writing software with LLM agents | Live and let Learn, accessed April 15, 2026, [https://liveandletlearn.net/post/10-tips-writing-software-llm-agents/](https://liveandletlearn.net/post/10-tips-writing-software-llm-agents/)  
31. Rust for LLM Framework in Enterprise AI Systems | by Yeahia Sarker | Feb, 2026 | Medium, accessed April 15, 2026, [https://medium.com/@yeahia.sarker/rust-for-llm-framework-in-enterprise-ai-systems-166a301a07fa](https://medium.com/@yeahia.sarker/rust-for-llm-framework-in-enterprise-ai-systems-166a301a07fa)