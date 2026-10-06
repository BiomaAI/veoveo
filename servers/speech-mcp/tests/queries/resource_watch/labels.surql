-- Change only the source's current required labels; portable Task state is untouched.
UPDATE $artifact SET labels = $labels;
