# Diplomacy, embassies and opinion

How players meet, make peace, break it, build embassies and decide whether they like each other. The pure rules are in `src/rules/diplomacy.rs` (read-only queries) and `src/rules/commands.rs` (the state changes). The engine drives them from `src/engine/apply.rs`, `src/engine/turn.rs`, `src/engine/vision.rs` and `src/engine/ai_diplomacy.rs`. The opinion model that the AI uses is `src/engine/opinion.rs`. The diplomacy commands exist from game version 60 (`diplomacy_commands_for` offers nothing below that). Tech names are in [reference/techs.md](reference/techs.md), the embassy numbers in [reference/tasks-and-diplomacy.md](reference/tasks-and-diplomacy.md).

## Relation data

Every ordered pair of players has a `DiplomacyRelation` (`src/types.rs`): `state`, `last_attack_turn`, `previous_attack_turn`, `last_peace_broken_turn`, `first_meet`, `embassy_level` and `embassy_build_turn`. The turn fields start at `DIPLOMACY_DEFAULT_TURN` (-100). The states are Neutral (0), Peace (1), War (2) and CoolDown (3).

War is not a stored state in practice. Only the pre-version-80 branch of `break_peace` sets `War`. Everywhere else, war is worked out from recent attacks (below).

## Meeting

A player must know another player before it can offer peace or build an embassy (`known_players` in the player's runtime record). `Engine::meet` (`src/engine/vision.rs`) adds the other player to the list, explores the other's capital if the meeting player has the `capitalvision` ability, records `first_meet` and pays a one-off reward of `2 * min(ceil(score / 1000), 5) + 1` stars, where `score` is the other player's current score. From version 123, in a two-player game, the reward is 2 stars higher when the other player is player 1.

Players meet when:

- one of them sees the other's unit, or the other's city, on a tile it explores (`meet_on_explore`; `check_tribe_connections` repeats this over every explored tile at the start of each turn while someone is still unknown);
- a peace message arrives from an unknown sender, or a peace request is answered;
- one builds an embassy (the host meets the builder, above version 105).

The order of the meetings, and so the order of `known_players`, follows retail's action stack. Exploring walks the area column by column (the centre in its place) and explores first tile first, each tile queuing the meeting with the owner of a unit on it and then of a city, which the stack runs in reverse (the city's owner first). The turn-start check queues the meetings of all explored tiles in tile order and the stack runs them last first. A player queued twice is met once.

Nature is never met. See [movement](movement.md#vision-and-fog) for exploration.

## War, attacks and cool-down

`has_war_with(a, b)` is true when either player attacked the other recently. An attack on this turn counts. An attack one turn ago counts only while the attacked player's seat has not yet moved this turn. Peace overrides all of it (unless the other id is `0`). Because either side's attack counts, a defender who fights back is at war with the attacker too.

`Engine::note_attack` (`src/engine/apply.rs`) is called for every attack on another player's unit or city, for a move onto a foreign city and for a capture. It records the attack turn on both relations and destroys the embassies between the two players. It ignores attacks on Nature or on oneself.

Breaking a peace (`Command::BreakPeace`, valid only while at peace with a known, living player) depends on the version:

- Up to version 79 the breaker's relation becomes War and the other side's becomes Neutral.
- From version 80 both relations become CoolDown and `last_peace_broken_turn` is set to this turn.

The breaker's units are spent, and those standing in the other player's territory are disbanded (refunded only up to version 110).

CoolDown has no fixed length. It ends when the owner of the relation ends a turn (`end_of_turn_effects` turns the ending player's CoolDown relations into Neutral), or earlier at the start of a turn in `check_diplomacy`: for the higher seat once the turn is past the break turn, for the lower seat one turn later. While it lasts:

- the attack scan (`rules::path::attack_options`) skips units of that player, and an infiltrator cannot enter their cities, exactly as at peace;
- from version 120 a unit cannot capture their cities (`Engine::can_capture`);
- `has_active_peace_treaty` in the damage formula accepts it above version 99, but the engine's own `battle_query_base` tests peace only (see [specs/combat.md](specs/combat.md)).

Effects of peace itself: units of a partner cannot be attacked, exert no zone of control and may be passed, partner tiles count as friendly for healing and upgrading, and embassy income doubles. See [movement](movement.md) and [combat](combat.md).

## Peace treaties

1. A player with the `peacetreaty` ability (the Shields tech in the current rules) sends `Command::PeaceTreaty { with }`. It is valid when the two know each other, are not at peace, the other is alive and the sender has no request waiting in the other's inbox. The command only puts a message in that inbox.
2. At the start of the other player's next turn, `check_diplomacy` (`src/engine/turn.rs`) empties the inbox, meets unknown senders (a dead sender is ignored from version 82) and turns each peace message into a `Deferred::PeaceRequest`, which becomes a pending trigger.
3. While a request is pending, the only legal commands are the two `Command::PeaceResponse { with, accept }` answers. Accepting calls `rules::commands::peace_response`, which sets both directions to Peace and refreshes routes and city connections. Refusing changes nothing.

`Command::BreakPeace` is offered whenever the two are at peace.

## Embassies

`Command::EstablishEmbassy { with }` builds an embassy in another player's capital:

- the builder needs the `embassy` ability (the Diplomacy tech, which also gives `capitalvision`);
- the target is the other player's capital city, which that player must still own (not checked up to version 79);
- the builder must not already have an embassy there and must have the stars. The cost is `4 + 2 * (embassies other players hold toward the builder)`, using `rules::diplomacy::get_embassy_count`. The legal-command check and the AI use a flat 5 up to version 113 (`Engine::embassy_cost`, which matches `embassy_cost` in the rules data), but applying the command always charges the formula.

A new embassy has level 1. The rules data has `embassy_max_level` 3, but nothing in the engine raises the level. Building one explores the tiles around the host's capital.

Embassies pay stars every turn: `embassy_income` (2) times the level, doubled while the two are at peace, and nothing at war (`rules::economy::embassy_income`). Both sides gain:

- the builder is paid at the start of its turn for standing on the host's capital, unless a foreign unit occupies it (`foreign_capital_work`);
- the host's capital adds the same amount to its own work (`Engine::embassy_income_for`, through `tile_work`).

So a level 1 embassy gives each side 2 stars a turn, or 4 at peace. An embassy is destroyed by any attack between the two players (capturing a city is one), or when a player is eliminated. See [economy](economy.md) for how income is added up.

## Opinion and the AI's choices

Opinion is only used by the AI. `Engine::opinion(player, other)` (`src/engine/opinion.rs`) is a total built from these reasons and is recomputed once after each command. Positive reasons are added and negative ones subtracted:

| Reason | Source |
| --- | --- |
| Common and different relation | Third players: both at war with the same player +2, both at peace with the same player +1, and one at war with a friend of the other -1 or -2; at most 2; a peace broken recently takes off `4 / (turns since + 1)` |
| Peaceful and aggression | Turns since the other last attacked (or since meeting), in steps of `LONGSIGHT` (4): up to +1 for a long quiet spell, a penalty after a recent attack |
| Embassy | +1 for each embassy between the two |
| Threatening and intrusive | The other's visible units near the player's cities (1 on the player's land, 3 in a city, 0.2 at peace, 0.5 on neutral land), and the other owning tiles there or the player's start tile |
| Winning and brave | How close the other is to winning under the game mode and its rank; dislike of whoever leads |
| Weak and powerful | The ratio of weighted military strength (attack, defence, movement and range), capped at 2: the AI thinks more of stronger opponents and less of weaker ones |
| Like | A bonus for human opponents, larger on the easier difficulties |
| Dislike | A share spread over opponents it is not at peace with, plus a further 0.5 for the least liked |

`aggression` is the truncated `-opinion * 1000` (a fixed 1000 before version 60). The AI uses these in `src/engine/ai_diplomacy.rs`:

- it accepts a peace request when the opinion of the sender is above `LOVE_LIMIT` (2.0), and offers peace when a hash draw multiplied by the opinion is above it (score 1000);
- it breaks peace when the opinion is below 0, scoring `-500 * opinion - 200 * (its units in the other's territory)`;
- it values an embassy by income against cost, and not at all at war or when aggression is above 5000.

The scoring side is described in [specs/ai-score.md](specs/ai-score.md) and the move driver in [ai](ai.md). In online game types (multiplayer, matchmaking, competitive) the opinion table is rebuilt from scratch each time, which changes how long a dead opponent is remembered.

## Resigning

`rules::commands::resign` exists, but no engine command calls it, so a player cannot resign through `Engine::apply`. A player who has lost every city (or, in the other death condition, every city or unit) is eliminated by `check_eliminations`; see [game modes and winning](game-modes.md).
