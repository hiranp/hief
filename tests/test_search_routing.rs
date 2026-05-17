use hief::router::{RetrievalStrategy, route_query};

#[test]
fn test_search_routing_symbol_queries_are_deterministic() {
    assert_eq!(
        route_query("src::router::route_query"),
        RetrievalStrategy::Deterministic { top_k: 10 }
    );
}

#[test]
fn test_search_routing_conceptual_queries_are_semantic() {
    assert_eq!(
        route_query("how does adaptive retrieval routing work"),
        RetrievalStrategy::Semantic {
            top_k: 15,
            rerank: true,
        }
    );
}

#[test]
fn test_search_routing_mixed_queries_are_hybrid() {
    assert_eq!(
        route_query("how does src::router::route_query work"),
        RetrievalStrategy::Hybrid {
            lexical_k: 10,
            semantic_k: 10,
            rrf_k: 60,
        }
    );
}

#[test]
fn test_search_routing_default_fallback_is_hybrid() {
    assert_eq!(
        route_query("   "),
        RetrievalStrategy::Hybrid {
            lexical_k: 10,
            semantic_k: 10,
            rrf_k: 60,
        }
    );
}
