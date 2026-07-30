import type { Component } from "svelte";
import {
  House,
  ShoppingCart,
  Utensils,
  Plug,
  Car,
  HeartPulse,
  Sparkles,
  Shirt,
  Film,
  Dumbbell,
  GraduationCap,
  Plane,
  Gift,
  Landmark,
  Receipt,
  Shield,
  CircleQuestionMark,
  Briefcase,
  Award,
  Laptop,
  TrendingUp,
  Building,
  RotateCcw,
  ArrowLeftRight,
  Tag,
} from "@lucide/svelte";

/** Mirrors scrat_domain::category::CATEGORY_ICONS — keep the two in sync. */
export const CATEGORY_ICONS: { key: string; component: Component }[] = [
  { key: "house", component: House },
  { key: "shopping-cart", component: ShoppingCart },
  { key: "utensils", component: Utensils },
  { key: "plug", component: Plug },
  { key: "car", component: Car },
  { key: "heart-pulse", component: HeartPulse },
  { key: "sparkles", component: Sparkles },
  { key: "shirt", component: Shirt },
  { key: "film", component: Film },
  { key: "dumbbell", component: Dumbbell },
  { key: "graduation-cap", component: GraduationCap },
  { key: "plane", component: Plane },
  { key: "gift", component: Gift },
  { key: "landmark", component: Landmark },
  { key: "receipt", component: Receipt },
  { key: "shield", component: Shield },
  { key: "circle-question-mark", component: CircleQuestionMark },
  { key: "briefcase", component: Briefcase },
  { key: "award", component: Award },
  { key: "laptop", component: Laptop },
  { key: "trending-up", component: TrendingUp },
  { key: "building", component: Building },
  { key: "rotate-ccw", component: RotateCcw },
  { key: "arrow-left-right", component: ArrowLeftRight },
  { key: "tag", component: Tag },
];

const iconByKey = new Map(CATEGORY_ICONS.map((i) => [i.key, i.component]));

/** Falls back to the generic "tag" icon for an unknown/missing key. */
export function iconComponentFor(key: string | null | undefined): Component {
  return (key && iconByKey.get(key)) || Tag;
}
