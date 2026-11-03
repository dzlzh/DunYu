"""Compare unsampled warm page transitions; rendering counters are inclusive."""
import argparse
import json
import math
from pathlib import Path


def p95(values):
    values = sorted(values)
    if not values:
        raise ValueError('No unsampled warm page transitions (indices 6..17)')
    return values[math.ceil(len(values) * 0.95) - 1]


def summarize(path):
    report = json.loads(Path(path).read_text(encoding='utf-8'))
    runs = [r for r in report['runs'] if not r['sampling'] and 6 <= r['index'] <= 17]
    return report, {
        'handler': p95([r['handler_ms'] for r in runs]),
        'layout': p95([r['counters'].get('layout', [0, 0])[1] for r in runs]),
        'paint': p95([r['counters'].get('paint', [0, 0])[1] for r in runs]),
        'paint.tab': p95([r['counters']['paint.tab'][1] for r in runs]) if all('paint.tab' in r['counters'] for r in runs) else None,
    }


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('baseline')
    parser.add_argument('candidate')
    parser.add_argument('--goal', choices=['transition', 'navigation'], default='transition')
    args = parser.parse_args()
    old, before = summarize(args.baseline)
    new, after = summarize(args.candidate)
    comparable = all(old.get(k) == new.get(k) for k in ['dpi', 'client_size', 'os', 'measurement_protocol'])
    if args.goal == 'navigation':
        goal_met = (before['paint.tab'] is not None and after['paint.tab'] is not None
                    and after['paint.tab'] <= before['paint.tab'] * 0.5
                    and after['paint'] <= before['paint'] * 0.8
                    and after['handler'] <= before['handler'] * 1.1)
    else:
        goal_met = after['handler'] <= before['handler'] * 0.7 and after['paint'] <= before['paint']
    print(json.dumps({
        'goal': args.goal,
        'matching_environment_metadata': comparable,
        'warm_p95_ms_before': before,
        'warm_p95_ms_after': after,
        'handler_reduction_percent': 100 * (1 - after['handler'] / before['handler']) if before['handler'] else None,
        'performance_goal_met': comparable and goal_met,
        'visual_failure_candidates': new.get('visual_failures', 'not recorded by this build'),
        'note': 'Use the same machine. Inclusive counters must not be added. Inspect sampling gaps and images; this is not a visual pass.'
    }, indent=2, ensure_ascii=False))
