import { useState, useEffect, useRef, useCallback } from "react";
import { searchQuery } from "../lib/ipc";
import type { SearchResult } from "../lib/types";

/**
 * `lang` is the language the backend is rendering in. Plugin results are
 * localized in Rust and arrive as finished strings, so a language switch
 * has to re-run the query: re-rendering repaints the shell around results
 * that are still worded in the language the user just left.
 */
export function useSearch(lang: string) {
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
  }, [query, nonce, lang]);

  return { query, setQuery, results, isLoading, refresh };
}
