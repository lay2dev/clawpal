import { useState, useEffect, useRef } from "react";
import { Input } from "@/components/ui/input";

interface AutocompleteFieldProps {
  value: string;
  onChange: (val: string) => void;
  onFocus?: () => void;
  options: { value: string; label: string }[];
  placeholder: string;
}

export function AutocompleteField({ value, onChange, onFocus, options, placeholder }: AutocompleteFieldProps) {
  const [open, setOpen] = useState(false);
  const wrapperRef = useRef<HTMLDivElement>(null);

  const filtered = options.filter(
    (o) => !value || o.value.toLowerCase().includes(value.toLowerCase()) || o.label.toLowerCase().includes(value.toLowerCase()),
  );

  useEffect(() => {
    function handleClickOutside(e: MouseEvent) {
      if (wrapperRef.current && !wrapperRef.current.contains(e.target as Node)) setOpen(false);
    }
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, []);

  return (
    <div ref={wrapperRef} className="relative">
      <Input
        placeholder={placeholder}
        value={value}
        onChange={(e) => { onChange(e.target.value); setOpen(true); }}
        onFocus={() => { setOpen(true); onFocus?.(); }}
        onKeyDown={(e) => { if (e.key === "Escape") setOpen(false); }}
      />
      {open && filtered.length > 0 && (
        <div className="absolute z-50 w-full mt-1 bg-popover border border-border rounded-md shadow-md max-h-[200px] overflow-y-auto">
          {filtered.map((option) => (
            <div key={option.value} className="px-3 py-1.5 text-sm cursor-pointer hover:bg-accent hover:text-accent-foreground"
              onMouseDown={(e) => { e.preventDefault(); onChange(option.value); setOpen(false); }}>
              {option.label}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
