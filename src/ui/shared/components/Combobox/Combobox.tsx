/* eslint-disable react/no-multi-comp -- Compound component wrappers belong together. */
import { Combobox as ComboboxPrimitive } from "@base-ui/react/combobox";
import { CheckIcon, ChevronDownIcon } from "lucide-react";

import { buttonVariants } from "@/shared/components/Button";
import { Input } from "@/shared/components/Input";
import { cn } from "@/shared/lib/utils";

const Combobox = ComboboxPrimitive.Root;

function ComboboxValue(props: ComboboxPrimitive.Value.Props) {
  return <ComboboxPrimitive.Value {...props} />;
}

function ComboboxTrigger({
  className,
  children,
  ...props
}: ComboboxPrimitive.Trigger.Props) {
  return (
    <ComboboxPrimitive.Trigger
      type="button"
      className={cn(
        buttonVariants({ variant: "outline" }),
        "w-full justify-between px-3 font-normal",
        className,
      )}
      {...props}
    >
      <span className="min-w-0 truncate text-left">{children}</span>
      <ComboboxPrimitive.Icon>
        <ChevronDownIcon className="size-4 text-muted-foreground" />
      </ComboboxPrimitive.Icon>
    </ComboboxPrimitive.Trigger>
  );
}

function ComboboxInput({ className, ...props }: ComboboxPrimitive.Input.Props) {
  return (
    <ComboboxPrimitive.Input
      render={<Input />}
      className={cn("m-2 w-[calc(100%-1rem)]", className)}
      {...props}
    />
  );
}

function ComboboxContent({
  className,
  children,
  ...props
}: ComboboxPrimitive.Popup.Props) {
  return (
    <ComboboxPrimitive.Portal>
      <ComboboxPrimitive.Positioner
        align="start"
        className="isolate z-50"
        sideOffset={4}
      >
        <ComboboxPrimitive.Popup
          className={cn(
            "max-h-[min(24rem,var(--available-height))] w-(--anchor-width) min-w-64 overflow-hidden rounded-md border border-border bg-popover text-popover-foreground shadow-md outline-none",
            className,
          )}
          {...props}
        >
          {children}
        </ComboboxPrimitive.Popup>
      </ComboboxPrimitive.Positioner>
    </ComboboxPrimitive.Portal>
  );
}

function ComboboxList({ className, ...props }: ComboboxPrimitive.List.Props) {
  return (
    <ComboboxPrimitive.List
      className={cn("max-h-72 overflow-y-auto p-1", className)}
      {...props}
    />
  );
}

function ComboboxItem({
  className,
  children,
  ...props
}: ComboboxPrimitive.Item.Props) {
  return (
    <ComboboxPrimitive.Item
      className={cn(
        "relative flex cursor-default items-start gap-2 rounded-sm py-2 pr-8 pl-2 text-sm outline-none select-none data-disabled:pointer-events-none data-disabled:opacity-50 data-highlighted:bg-accent data-highlighted:text-accent-foreground",
        className,
      )}
      {...props}
    >
      {children}
      <ComboboxPrimitive.ItemIndicator className="absolute top-2.5 right-2">
        <CheckIcon className="size-4" />
      </ComboboxPrimitive.ItemIndicator>
    </ComboboxPrimitive.Item>
  );
}

export {
  Combobox,
  ComboboxContent,
  ComboboxInput,
  ComboboxItem,
  ComboboxList,
  ComboboxTrigger,
  ComboboxValue,
};
