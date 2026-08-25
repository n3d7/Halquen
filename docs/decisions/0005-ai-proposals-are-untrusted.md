# ADR 0005: AI proposals are untrusted

Status: accepted

Future model output is represented as a proposal, not a mutation or permission. AI inference and
external content cannot independently promote procedural memory. Persistence resolves exactly the
IDs referenced by a revision in its transaction; unrelated trusted evidence cannot authorize an
untrusted revision. Only referenced explicit or confirmed user evidence can make a procedural
candidate eligible for normal capability and policy review.
