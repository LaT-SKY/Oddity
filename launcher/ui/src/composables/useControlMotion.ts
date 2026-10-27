import { computed } from "vue";
import { useReducedMotion, useMotionConfig } from "motion-v";

export const controlSpring = {
  type: "spring" as const,
  stiffness: 480,
  damping: 34,
  mass: 0.8,
};
export const surfaceSpring = {
  type: "spring" as const,
  stiffness: 380,
  damping: 34,
  mass: 0.9,
};

export function useControlMotion() {
  const prefersReduced = useReducedMotion();
  const config = useMotionConfig();
  const reduced = computed(
    () => prefersReduced.value || config.value.reducedMotion === "always",
  );
  return {
    reduced,
    control: computed(() => (reduced.value ? { duration: 0 } : controlSpring)),
    surface: computed(() => (reduced.value ? { duration: 0 } : surfaceSpring)),
  };
}
