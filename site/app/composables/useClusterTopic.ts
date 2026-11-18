/**
 * Which topic the cluster section shows. The row under the terminal sets it, so choosing
 * "Shared volumes" there opens that tab further down the page.
 */
export function useClusterTopic() {
  return useState("cluster-topic", () => "network");
}
