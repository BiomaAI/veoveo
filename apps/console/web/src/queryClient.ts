import { QueryClient } from "@tanstack/react-query";
import { AuthenticationRequiredError } from "./auth";

export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: (failureCount, error) =>
        !(error instanceof AuthenticationRequiredError) && failureCount < 1,
      refetchOnWindowFocus: false,
      staleTime: 30_000
    }
  }
});
