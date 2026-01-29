import { Hardcoded } from '../components/Hardcoded';
import { MissingKeys } from '../components/MissingKeys';
import { UnresolvedKeys } from '../components/UnresolvedKeys';
import { ParseError } from '../components/ParseError';

export default function Page() {
  return (
    <main>
      <Hardcoded />
      <MissingKeys />
      <UnresolvedKeys />
      <ParseError />
    </main>
  );
}
