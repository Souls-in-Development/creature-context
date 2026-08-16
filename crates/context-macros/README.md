# Creature Context Macros

This crate elevates the Creature Context `.idx` metadata from a passive descriptor into the **live architectural grammar** of the codebase. 

By integrating `#[context_enforce]` as a procedural macro, the Rust compiler explicitly queries the `ATLAS.idx` graph during compilation. If a module attempts an import, dependency, or structural change that contradicts the explicit rules of the context graph (e.g., a cross-boundary violation), the compilation physically halts.

This enforces epistemological hygiene: the AI (and human developers) must respect the architectural language, and the compiler serves as the ultimate arbiter of that language.
