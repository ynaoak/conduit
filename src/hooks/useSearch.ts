import { useState, useEffect, useRef, useCallback } from "react";
import { searchQuery } from "../lib/ipc";
import type { SearchResult } from "../lib/types";

export function useSearch() {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SearchResult[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [nonce, setNonce] = useState(0);
  const debounceRef = useRef<ReturnType<typeof setTimeout>>(undefined);

  /** Re-run the current query (e.g. after deleting a clipboard entry) */
  const refresh = useCallback(() => setNonce((n) => n + 1), []);

  useEffect(() => {
    // Empty query still hits the backend: plugins return the default
    // dashboard view (favorite apps, open windows, recent clips)
    setIsLoading(true);
    clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(async () => {
      try {
        const res = await searchQuery(query);
        setResults(res);
      } catch (err) {
        console.error("Search failed:", err);
        setResults([]);
      } finally {
        setIsLoading(false);
      }
    }, 80);

    return () => clearTimeout(debounceRef.current);
  }, [query, nonce]);

  return { query, setQuery, results, isLoading, refresh };
}
