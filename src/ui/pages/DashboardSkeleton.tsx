import { Skeleton } from "@/shared/components";

/** Renders a layout-preserving operational dashboard loading state. @returns The dashboard skeleton. */
export function DashboardSkeleton() {
  return (
    <section className="flex flex-col gap-6">
      <Skeleton className="h-8 w-64" />
      <div className="grid grid-cols-2 gap-4 sm:grid-cols-4">
        {[1, 2, 3, 4].map((item) => (
          <Skeleton className="h-32" key={item} />
        ))}
      </div>
      <Skeleton className="h-48" />
    </section>
  );
}
