-- Marks each category with the stable key of the built-in category it was
-- seeded from, so the app can tell "a default we created" from "a category
-- the user made" — and, across a language change, "still called what we
-- named it" from "the user has renamed this".
--
-- NULL means user-created (or user-renamed before this column existed) and is
-- the correct answer for anything the app should never touch again.
ALTER TABLE categories ADD COLUMN seed_key TEXT;

-- Backfill for databases created before the column existed. Matching on the
-- English name is exactly the question being asked: this database was seeded
-- in English, so a seeded category still carrying its seeded name has not
-- been made the user's own, and re-adopting it is safe. Anything renamed
-- stays NULL and is left alone forever, which is the conservative outcome.
--
-- Parents are matched first, then children by their parent's freshly written
-- key: subcategory names are not unique on their own ('Insurance > Travel'
-- and the top-level 'Travel'), so position is part of the identity.
--
-- Generated from crates/domain/src/default_categories.rs — see the key
-- stability note there. Do not hand-edit; add a new migration instead.

UPDATE categories SET seed_key = 'food_and_drink'
 WHERE seed_key IS NULL AND parent_id IS NULL AND name = 'Food & Drink';
UPDATE categories SET seed_key = 'food_and_drink.restaurant'
 WHERE seed_key IS NULL AND name = 'Restaurant'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'food_and_drink');
UPDATE categories SET seed_key = 'food_and_drink.bar'
 WHERE seed_key IS NULL AND name = 'Bar'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'food_and_drink');

UPDATE categories SET seed_key = 'groceries'
 WHERE seed_key IS NULL AND parent_id IS NULL AND name = 'Groceries';

UPDATE categories SET seed_key = 'housing'
 WHERE seed_key IS NULL AND parent_id IS NULL AND name = 'Housing';
UPDATE categories SET seed_key = 'housing.rent'
 WHERE seed_key IS NULL AND name = 'Rent'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'housing');
UPDATE categories SET seed_key = 'housing.mortgage'
 WHERE seed_key IS NULL AND name = 'Mortgage'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'housing');
UPDATE categories SET seed_key = 'housing.maintenance'
 WHERE seed_key IS NULL AND name = 'Maintenance'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'housing');
UPDATE categories SET seed_key = 'housing.furniture'
 WHERE seed_key IS NULL AND name = 'Furniture'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'housing');
UPDATE categories SET seed_key = 'housing.appliances'
 WHERE seed_key IS NULL AND name = 'Appliances'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'housing');
UPDATE categories SET seed_key = 'housing.home_decor'
 WHERE seed_key IS NULL AND name = 'Home Decor'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'housing');

UPDATE categories SET seed_key = 'utilities'
 WHERE seed_key IS NULL AND parent_id IS NULL AND name = 'Utilities';
UPDATE categories SET seed_key = 'utilities.electricity'
 WHERE seed_key IS NULL AND name = 'Electricity'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'utilities');
UPDATE categories SET seed_key = 'utilities.water'
 WHERE seed_key IS NULL AND name = 'Water'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'utilities');
UPDATE categories SET seed_key = 'utilities.gas'
 WHERE seed_key IS NULL AND name = 'Gas'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'utilities');
UPDATE categories SET seed_key = 'utilities.internet'
 WHERE seed_key IS NULL AND name = 'Internet'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'utilities');
UPDATE categories SET seed_key = 'utilities.mobile_phone'
 WHERE seed_key IS NULL AND name = 'Mobile Phone'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'utilities');
UPDATE categories SET seed_key = 'utilities.tv_and_streaming'
 WHERE seed_key IS NULL AND name = 'TV & Streaming'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'utilities');

UPDATE categories SET seed_key = 'transportation'
 WHERE seed_key IS NULL AND parent_id IS NULL AND name = 'Transportation';
UPDATE categories SET seed_key = 'transportation.fuel'
 WHERE seed_key IS NULL AND name = 'Fuel'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'transportation');
UPDATE categories SET seed_key = 'transportation.public_transit'
 WHERE seed_key IS NULL AND name = 'Public Transit'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'transportation');
UPDATE categories SET seed_key = 'transportation.taxi_and_rideshare'
 WHERE seed_key IS NULL AND name = 'Taxi & Rideshare'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'transportation');
UPDATE categories SET seed_key = 'transportation.parking'
 WHERE seed_key IS NULL AND name = 'Parking'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'transportation');
UPDATE categories SET seed_key = 'transportation.tolls'
 WHERE seed_key IS NULL AND name = 'Tolls'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'transportation');
UPDATE categories SET seed_key = 'transportation.vehicle_maintenance'
 WHERE seed_key IS NULL AND name = 'Vehicle Maintenance'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'transportation');

UPDATE categories SET seed_key = 'healthcare'
 WHERE seed_key IS NULL AND parent_id IS NULL AND name = 'Healthcare';
UPDATE categories SET seed_key = 'healthcare.doctor'
 WHERE seed_key IS NULL AND name = 'Doctor'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'healthcare');
UPDATE categories SET seed_key = 'healthcare.pharmacy'
 WHERE seed_key IS NULL AND name = 'Pharmacy'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'healthcare');

UPDATE categories SET seed_key = 'personal_care'
 WHERE seed_key IS NULL AND parent_id IS NULL AND name = 'Personal Care';
UPDATE categories SET seed_key = 'personal_care.haircuts'
 WHERE seed_key IS NULL AND name = 'Haircuts'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'personal_care');
UPDATE categories SET seed_key = 'personal_care.cosmetics'
 WHERE seed_key IS NULL AND name = 'Cosmetics'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'personal_care');
UPDATE categories SET seed_key = 'personal_care.skincare'
 WHERE seed_key IS NULL AND name = 'Skincare'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'personal_care');
UPDATE categories SET seed_key = 'personal_care.hygiene'
 WHERE seed_key IS NULL AND name = 'Hygiene'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'personal_care');

UPDATE categories SET seed_key = 'clothing'
 WHERE seed_key IS NULL AND parent_id IS NULL AND name = 'Clothing';
UPDATE categories SET seed_key = 'clothing.clothes'
 WHERE seed_key IS NULL AND name = 'Clothes'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'clothing');
UPDATE categories SET seed_key = 'clothing.shoes'
 WHERE seed_key IS NULL AND name = 'Shoes'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'clothing');
UPDATE categories SET seed_key = 'clothing.accessories'
 WHERE seed_key IS NULL AND name = 'Accessories'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'clothing');

UPDATE categories SET seed_key = 'entertainment'
 WHERE seed_key IS NULL AND parent_id IS NULL AND name = 'Entertainment';
UPDATE categories SET seed_key = 'entertainment.movies'
 WHERE seed_key IS NULL AND name = 'Movies'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'entertainment');
UPDATE categories SET seed_key = 'entertainment.concerts'
 WHERE seed_key IS NULL AND name = 'Concerts'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'entertainment');
UPDATE categories SET seed_key = 'entertainment.games'
 WHERE seed_key IS NULL AND name = 'Games'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'entertainment');
UPDATE categories SET seed_key = 'entertainment.hobbies'
 WHERE seed_key IS NULL AND name = 'Hobbies'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'entertainment');
UPDATE categories SET seed_key = 'entertainment.events'
 WHERE seed_key IS NULL AND name = 'Events'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'entertainment');

UPDATE categories SET seed_key = 'sports_and_fitness'
 WHERE seed_key IS NULL AND parent_id IS NULL AND name = 'Sports & Fitness';
UPDATE categories SET seed_key = 'sports_and_fitness.gym'
 WHERE seed_key IS NULL AND name = 'Gym'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'sports_and_fitness');

UPDATE categories SET seed_key = 'education'
 WHERE seed_key IS NULL AND parent_id IS NULL AND name = 'Education';
UPDATE categories SET seed_key = 'education.books'
 WHERE seed_key IS NULL AND name = 'Books'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'education');
UPDATE categories SET seed_key = 'education.courses'
 WHERE seed_key IS NULL AND name = 'Courses'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'education');

UPDATE categories SET seed_key = 'travel'
 WHERE seed_key IS NULL AND parent_id IS NULL AND name = 'Travel';
UPDATE categories SET seed_key = 'travel.flights'
 WHERE seed_key IS NULL AND name = 'Flights'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'travel');
UPDATE categories SET seed_key = 'travel.accommodation'
 WHERE seed_key IS NULL AND name = 'Accommodation'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'travel');
UPDATE categories SET seed_key = 'travel.trains'
 WHERE seed_key IS NULL AND name = 'Trains'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'travel');
UPDATE categories SET seed_key = 'travel.car_rental'
 WHERE seed_key IS NULL AND name = 'Car Rental'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'travel');
UPDATE categories SET seed_key = 'travel.activities'
 WHERE seed_key IS NULL AND name = 'Activities'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'travel');

UPDATE categories SET seed_key = 'gifts_and_donations'
 WHERE seed_key IS NULL AND parent_id IS NULL AND name = 'Gifts & Donations';
UPDATE categories SET seed_key = 'gifts_and_donations.gifts'
 WHERE seed_key IS NULL AND name = 'Gifts'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'gifts_and_donations');

UPDATE categories SET seed_key = 'financial'
 WHERE seed_key IS NULL AND parent_id IS NULL AND name = 'Financial';
UPDATE categories SET seed_key = 'financial.bank_fees'
 WHERE seed_key IS NULL AND name = 'Bank Fees'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'financial');
UPDATE categories SET seed_key = 'financial.loan_payments'
 WHERE seed_key IS NULL AND name = 'Loan Payments'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'financial');

UPDATE categories SET seed_key = 'taxes_and_government'
 WHERE seed_key IS NULL AND parent_id IS NULL AND name = 'Taxes & Government';
UPDATE categories SET seed_key = 'taxes_and_government.income_tax'
 WHERE seed_key IS NULL AND name = 'Income Tax'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'taxes_and_government');
UPDATE categories SET seed_key = 'taxes_and_government.property_tax'
 WHERE seed_key IS NULL AND name = 'Property Tax'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'taxes_and_government');
UPDATE categories SET seed_key = 'taxes_and_government.vehicle_tax'
 WHERE seed_key IS NULL AND name = 'Vehicle Tax'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'taxes_and_government');
UPDATE categories SET seed_key = 'taxes_and_government.government_fees'
 WHERE seed_key IS NULL AND name = 'Government Fees'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'taxes_and_government');

UPDATE categories SET seed_key = 'insurance'
 WHERE seed_key IS NULL AND parent_id IS NULL AND name = 'Insurance';
UPDATE categories SET seed_key = 'insurance.health'
 WHERE seed_key IS NULL AND name = 'Health'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'insurance');
UPDATE categories SET seed_key = 'insurance.home'
 WHERE seed_key IS NULL AND name = 'Home'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'insurance');
UPDATE categories SET seed_key = 'insurance.vehicle'
 WHERE seed_key IS NULL AND name = 'Vehicle'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'insurance');
UPDATE categories SET seed_key = 'insurance.life'
 WHERE seed_key IS NULL AND name = 'Life'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'insurance');
UPDATE categories SET seed_key = 'insurance.travel'
 WHERE seed_key IS NULL AND name = 'Travel'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'insurance');

UPDATE categories SET seed_key = 'uncategorized'
 WHERE seed_key IS NULL AND parent_id IS NULL AND name = 'Uncategorized';

UPDATE categories SET seed_key = 'salary'
 WHERE seed_key IS NULL AND parent_id IS NULL AND name = 'Salary';
UPDATE categories SET seed_key = 'salary.base_salary'
 WHERE seed_key IS NULL AND name = 'Base Salary'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'salary');
UPDATE categories SET seed_key = 'salary.overtime'
 WHERE seed_key IS NULL AND name = 'Overtime'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'salary');
UPDATE categories SET seed_key = 'salary.commission'
 WHERE seed_key IS NULL AND name = 'Commission'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'salary');

UPDATE categories SET seed_key = 'bonus'
 WHERE seed_key IS NULL AND parent_id IS NULL AND name = 'Bonus';
UPDATE categories SET seed_key = 'bonus.performance'
 WHERE seed_key IS NULL AND name = 'Performance'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'bonus');
UPDATE categories SET seed_key = 'bonus.holiday'
 WHERE seed_key IS NULL AND name = 'Holiday'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'bonus');
UPDATE categories SET seed_key = 'bonus.referral'
 WHERE seed_key IS NULL AND name = 'Referral'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'bonus');

UPDATE categories SET seed_key = 'freelance_and_business'
 WHERE seed_key IS NULL AND parent_id IS NULL AND name = 'Freelance & Business';
UPDATE categories SET seed_key = 'freelance_and_business.client_payments'
 WHERE seed_key IS NULL AND name = 'Client Payments'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'freelance_and_business');
UPDATE categories SET seed_key = 'freelance_and_business.product_sales'
 WHERE seed_key IS NULL AND name = 'Product Sales'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'freelance_and_business');
UPDATE categories SET seed_key = 'freelance_and_business.service_income'
 WHERE seed_key IS NULL AND name = 'Service Income'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'freelance_and_business');

UPDATE categories SET seed_key = 'investment_income'
 WHERE seed_key IS NULL AND parent_id IS NULL AND name = 'Investment Income';
UPDATE categories SET seed_key = 'investment_income.dividends'
 WHERE seed_key IS NULL AND name = 'Dividends'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'investment_income');
UPDATE categories SET seed_key = 'investment_income.interest'
 WHERE seed_key IS NULL AND name = 'Interest'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'investment_income');
UPDATE categories SET seed_key = 'investment_income.capital_gains'
 WHERE seed_key IS NULL AND name = 'Capital Gains'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'investment_income');

UPDATE categories SET seed_key = 'rental_income'
 WHERE seed_key IS NULL AND parent_id IS NULL AND name = 'Rental Income';
UPDATE categories SET seed_key = 'rental_income.property_rent'
 WHERE seed_key IS NULL AND name = 'Property Rent'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'rental_income');

UPDATE categories SET seed_key = 'government_benefits'
 WHERE seed_key IS NULL AND parent_id IS NULL AND name = 'Government Benefits';
UPDATE categories SET seed_key = 'government_benefits.pension'
 WHERE seed_key IS NULL AND name = 'Pension'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'government_benefits');
UPDATE categories SET seed_key = 'government_benefits.unemployment'
 WHERE seed_key IS NULL AND name = 'Unemployment'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'government_benefits');
UPDATE categories SET seed_key = 'government_benefits.child_benefits'
 WHERE seed_key IS NULL AND name = 'Child Benefits'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'government_benefits');
UPDATE categories SET seed_key = 'government_benefits.social_assistance'
 WHERE seed_key IS NULL AND name = 'Social Assistance'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'government_benefits');

UPDATE categories SET seed_key = 'refunds_and_reimbursements'
 WHERE seed_key IS NULL AND parent_id IS NULL AND name = 'Refunds & Reimbursements';
UPDATE categories SET seed_key = 'refunds_and_reimbursements.purchase_refund'
 WHERE seed_key IS NULL AND name = 'Purchase Refund'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'refunds_and_reimbursements');
UPDATE categories SET seed_key = 'refunds_and_reimbursements.tax_refund'
 WHERE seed_key IS NULL AND name = 'Tax Refund'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'refunds_and_reimbursements');
UPDATE categories SET seed_key = 'refunds_and_reimbursements.insurance_reimbursement'
 WHERE seed_key IS NULL AND name = 'Insurance Reimbursement'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'refunds_and_reimbursements');
UPDATE categories SET seed_key = 'refunds_and_reimbursements.expense_reimbursement'
 WHERE seed_key IS NULL AND name = 'Expense Reimbursement'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'refunds_and_reimbursements');

UPDATE categories SET seed_key = 'transfers'
 WHERE seed_key IS NULL AND parent_id IS NULL AND name = 'Transfers';
UPDATE categories SET seed_key = 'transfers.investment_transfers'
 WHERE seed_key IS NULL AND name = 'Investment Transfers'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'transfers');
UPDATE categories SET seed_key = 'transfers.credit_card_payments'
 WHERE seed_key IS NULL AND name = 'Credit Card Payments'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'transfers');
UPDATE categories SET seed_key = 'transfers.cash_withdrawal'
 WHERE seed_key IS NULL AND name = 'Cash Withdrawal'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'transfers');
UPDATE categories SET seed_key = 'transfers.cash_deposit'
 WHERE seed_key IS NULL AND name = 'Cash Deposit'
   AND parent_id IN (SELECT id FROM categories WHERE seed_key = 'transfers');
