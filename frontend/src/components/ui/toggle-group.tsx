import * as React from "react"
import { cn } from "@/lib/utils"
import { ToggleGroup as ToggleGroupPrimitive } from "radix-ui"

/** One choice out of a few, as a row of buttons joined in a frame. */
function ToggleGroup({ className, ...props }: Omit<React.ComponentProps<"div">, "dir" | "defaultValue" | "onChange"> & { value?: string; onValueChange?: (value: string) => void; disabled?: boolean }) {
  return (
    <ToggleGroupPrimitive.Root
      type="single"
      data-slot="toggle-group"
      className={cn("inline-flex rounded-md border bg-card p-0.5", className)}
      {...props}
    />
  )
}

function ToggleGroupItem({ className, ...props }: React.ComponentProps<typeof ToggleGroupPrimitive.Item>) {
  return (
    <ToggleGroupPrimitive.Item
      data-slot="toggle-group-item"
      className={cn(
        "grid size-8 place-items-center rounded-sm text-muted-foreground transition-colors outline-none hover:text-foreground focus-visible:ring-[3px] focus-visible:ring-ring/50 data-[state=on]:bg-accent data-[state=on]:text-foreground [&_svg:not([class*='size-'])]:size-4",
        className
      )}
      {...props}
    />
  )
}

export { ToggleGroup, ToggleGroupItem }
