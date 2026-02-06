import type { AppDefinition } from '@/types/generated';

interface IconProps {
  app: AppDefinition;
  onOpen: (app: AppDefinition) => void;
}

export function Icon({ app, onOpen }: IconProps) {
  return (
    <button className="desktop-icon" type="button" onClick={() => onOpen(app)} title={app.name}>
      <span className="desktop-icon__emoji">{app.icon}</span>
      <span className="desktop-icon__label">{app.name}</span>
    </button>
  );
}
