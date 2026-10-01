-- Profile selectors are bound data. Templates consume the first next delimiter;
-- later delimiters cannot rescue an empty variable or an unmatched suffix.
LET $resource_selected = |$collection: string, $uri: string, $selections: array| {
    RETURN $selections.any(|$selection| {
        IF $selection.collection_id != $collection
            OR string::starts_with($uri, string::concat($selection.scheme, '://')) = false {
            RETURN false;
        };
        RETURN $selection.selectors.any(|$selector| {
            IF $selector.kind = 'scheme' {
                RETURN string::starts_with($uri, string::concat($selector.scheme, '://'));
            };
            IF $selector.kind = 'prefix' {
                RETURN string::starts_with($uri, $selector.prefix);
            };
            IF $selector.kind != 'template' { RETURN false; };
            LET $remaining = $selector.literals.fold($uri, |$rest, $literal, $index| {
                IF $rest = NONE { RETURN NONE; };
                IF $index = 0 {
                    RETURN IF string::starts_with($rest, $literal) {
                        string::slice($rest, string::len($literal), string::len($rest))
                    } ELSE { NONE };
                };
                IF $literal = '' { RETURN IF $rest != '' { '' } ELSE { NONE }; };
                LET $parts = string::split($rest, $literal);
                IF array::len($parts) < 2 OR $parts[0] = '' { RETURN NONE; };
                RETURN string::slice($rest, string::len($parts[0]) + string::len($literal), string::len($rest));
            });
            RETURN $remaining = '';
        });
    });
};
