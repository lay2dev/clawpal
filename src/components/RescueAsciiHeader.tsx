import type { RescueBotRuntimeState } from "@/lib/types";
import { cn } from "@/lib/utils";
import doctorImage from "@/assets/doctor.png";

interface RescueAsciiHeaderProps {
  state: RescueBotRuntimeState;
  title: string;
  progress?: number;
  animateProgress?: boolean;
  animateFace?: boolean;
}

function clampProgress(progress?: number): number {
  if (typeof progress !== "number" || Number.isNaN(progress)) {
    return 0;
  }
  return Math.max(0, Math.min(1, progress));
}

const progressToneByState: Record<RescueBotRuntimeState, string> = {
  unconfigured: "bg-[#B38A54]",
  configured_inactive: "bg-[#B38A54]",
  active: "bg-[#78A287]",
  checking: "bg-[#C97A1A]",
  error: "bg-[#C65A3A]",
};

const PROGRESS_SLOTS = 14;

export function RescueAsciiHeader({
  state,
  title,
  progress,
  animateProgress = false,
  animateFace = false,
}: RescueAsciiHeaderProps) {
  const clampedProgress = clampProgress(progress);
  const filledSlots = Math.max(1, Math.round(clampedProgress * PROGRESS_SLOTS));

  return (
    <div className="min-w-0 text-center">
      <div
        role="img"
        aria-label={title}
        title={title}
        className="mx-auto w-[264px] sm:w-[312px]"
      >
        <img
          src={doctorImage}
          alt={title}
          className={cn(
            "h-auto w-full select-none object-contain",
            animateFace && "animate-pulse",
          )}
          draggable={false}
        />
      </div>
      <div className="mx-auto mt-2 h-[12px] w-[168px] sm:h-[13px] sm:w-[196px]">
        {animateProgress ? (
          <div className="flex justify-center gap-[3px]">
            {Array.from({ length: PROGRESS_SLOTS }).map((_, index) => {
              const filled = index < filledSlots;
              return (
                <span
                  key={index}
                  aria-hidden="true"
                  className={cn(
                    "inline-block h-[8px] w-[8px] rounded-[2px] transition-colors duration-300 sm:h-[9px] sm:w-[9px]",
                    filled ? progressToneByState[state] : "bg-[#E9DED2]",
                    filled && animateProgress && "animate-pulse",
                  )}
                />
              );
            })}
          </div>
        ) : null}
      </div>
    </div>
  );
}
